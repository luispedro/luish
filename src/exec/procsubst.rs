//! Process substitution, `<(...)`, `>(...)` and `=(...)`.
//!
//! For `<(...)` and `>(...)`, the list runs in a forked process with its
//! standard output (or input) on a pipe, and the word becomes `/dev/fd/N`,
//! N being the shell's end of the pipe. The shell keeps that end open until
//! the command it belongs to (`Shell::run_command`) is done. For `=(...)`,
//! the list's output goes to a temporary file, which is removed then.

use crate::ast::{List, ProcKind};
use crate::exec::redirect::clear_cloexec;
use crate::shell::{Flow, Shell};
use crate::sys;

/// A substitution whose command isn't over.
pub enum ProcSub {
    /// `<(...)` or `>(...)`: the shell's end of the pipe, and the process at
    /// the other end.
    Pipe {
        fd: i32,
        pid: i32,
        /// `>(...)`, whose process reads what the command writes.
        output: bool,
    },
    /// `=(...)`: the path of the temporary file.
    File(Vec<u8>),
}

/// In a child: closes the shell's ends of the pipes of the substitutions
/// (a `>(...)` would never see the end of its input), and forgets the
/// files, which are the parent's to remove.
fn forget_procsubs(procsubs: &mut Vec<ProcSub>) {
    for p in procsubs.drain(..) {
        if let ProcSub::Pipe { fd, .. } = p {
            sys::close(fd);
        }
    }
}

/// Moves `fd` to a number from 64 up, without close-on-exec (`fd` itself
/// if that fails, as when the limit on descriptors is low).
fn high_fd(fd: i32) -> i32 {
    // SAFETY: plain fcntl.
    let new = unsafe { libc::fcntl(fd, libc::F_DUPFD, 64) };
    if new < 0 {
        clear_cloexec(fd);
        return fd;
    }
    sys::close(fd);
    new
}

impl Shell {
    /// Starts `list` and returns the path of the pipe to it, or (`=(...)`)
    /// runs it and returns the path of the file with its output.
    pub fn process_subst(&mut self, kind: ProcKind, list: &List) -> Result<Vec<u8>, Flow> {
        if kind == ProcKind::File {
            return self.file_procsubst(list);
        }
        let output = kind == ProcKind::Output;
        self.reap_procsub_orphans();
        let (r, w) = match sys::pipe() {
            Ok(p) => p,
            Err(e) => {
                self.error(format!("Pipe call failed: {}", sys::strerror(e)));
                return Err(Flow::Error(2));
            }
        };
        // The list's end, and ours.
        let (theirs, ours) = if output { (r, w) } else { (w, r) };
        let pid = match self.fork_or_error() {
            Ok(pid) => pid,
            Err(e) => {
                sys::close(r);
                sys::close(w);
                return Err(e);
            }
        };
        if pid == 0 {
            forget_procsubs(&mut self.procsubs);
            sys::close(ours);
            let target = if output { 0 } else { 1 };
            if theirs != target {
                let _ = sys::dup2(theirs, target);
                sys::close(theirs);
            } else {
                clear_cloexec(theirs);
            }
            // As for `$(...)`, a condition around it doesn't suppress `set -e`.
            self.errexit_suppressed = 0;
            let res = self.run_list_exit(list, true);
            self.child_exit(res);
        }
        sys::close(theirs);
        // The command, and what it runs, must inherit the descriptor. It is
        // moved out of the way of the low numbers that scripts use for their
        // own redirections (`exec 3< <(cmd)` would close its own descriptor).
        let ours = high_fd(ours);
        self.procsubs.push(ProcSub::Pipe { fd: ours, pid, output });
        Ok(format!("/dev/fd/{ours}").into_bytes())
    }

    /// `=(list)`: runs `list` with its output to a new temporary file, and
    /// returns the file's path. As in zsh, the list's status is ignored.
    fn file_procsubst(&mut self, list: &List) -> Result<Vec<u8>, Flow> {
        let (fd, path) = self.temp_file(b"luish-")?;
        // Recorded now, so that the file is removed whatever happens next.
        self.procsubs.push(ProcSub::File(path.clone()));
        let pid = match self.fork_or_error() {
            Ok(pid) => pid,
            Err(e) => {
                sys::close(fd);
                return Err(e);
            }
        };
        if pid == 0 {
            forget_procsubs(&mut self.procsubs);
            if fd != 1 {
                let _ = sys::dup2(fd, 1);
                sys::close(fd);
            } else {
                clear_cloexec(fd);
            }
            // As for `$(...)`, a condition around it doesn't suppress `set -e`.
            self.errexit_suppressed = 0;
            let res = self.run_list_exit(list, true);
            self.child_exit(res);
        }
        sys::close(fd);
        self.wait_for(pid);
        Ok(path)
    }

    /// Closes the pipes of the substitutions after the first `mark`, as
    /// their command is over, and removes their files. A `>(...)` process is
    /// waited for, so that its output comes before whatever runs next. A
    /// `<(...)` process ends by itself or on SIGPIPE, and isn't waited for.
    pub fn end_procsubs(&mut self, mark: usize) {
        let done = self.procsubs.split_off(mark);
        for p in &done {
            match p {
                ProcSub::Pipe { fd, .. } => sys::close(*fd),
                ProcSub::File(path) => sys::unlink(path),
            }
        }
        for p in done {
            match p {
                ProcSub::Pipe { pid, output: true, .. } => {
                    self.wait_for(pid);
                }
                ProcSub::Pipe { pid, .. } => self.procsub_orphans.push(pid),
                ProcSub::File(_) => {}
            }
        }
        self.reap_procsub_orphans();
    }

    /// Closes our ends of the pipes after the first `mark`, without waiting
    /// (`exec 3< <(cmd)` has made copies), and removes the files (`exec 3<
    /// =(cmd)` has opened them).
    pub fn detach_procsubs(&mut self, mark: usize) {
        for p in self.procsubs.split_off(mark) {
            match p {
                ProcSub::Pipe { fd, pid, .. } => {
                    sys::close(fd);
                    self.procsub_orphans.push(pid);
                }
                ProcSub::File(path) => sys::unlink(&path),
            }
        }
    }

    fn reap_procsub_orphans(&mut self) {
        self.procsub_orphans
            .retain(|&pid| matches!(sys::waitpid(pid, libc::WNOHANG), Ok(None)));
    }
}
