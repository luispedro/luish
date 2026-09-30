//! Process substitution, `<(...)` and `>(...)`.
//!
//! The list runs in a forked process with its standard output (or input)
//! on a pipe, and the word becomes `/dev/fd/N`, N being the shell's end of
//! the pipe. The shell keeps that end open until the command it belongs to
//! (`Shell::run_command`) is done.

use crate::ast::List;
use crate::exec::redirect::clear_cloexec;
use crate::shell::{Flow, Shell};
use crate::sys;

/// A substitution that is running: the shell's end of the pipe, and the
/// process at the other end.
pub struct ProcSub {
    fd: i32,
    pid: i32,
    /// `>(...)`, whose process reads what the command writes.
    output: bool,
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
    /// Starts `list` and returns the path of the pipe to it.
    pub fn process_subst(&mut self, output: bool, list: &List) -> Result<Vec<u8>, Flow> {
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
            // Other substitutions' pipes mustn't be held open here: a `>(...)`
            // would never see the end of its input.
            for p in self.procsubs.drain(..) {
                sys::close(p.fd);
            }
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
        self.procsubs.push(ProcSub { fd: ours, pid, output });
        Ok(format!("/dev/fd/{ours}").into_bytes())
    }

    /// Closes the pipes of the substitutions after the first `mark`, as
    /// their command is over. A `>(...)` process is waited for, so that its
    /// output comes before whatever runs next. A `<(...)` process ends by
    /// itself or on SIGPIPE, and isn't waited for.
    pub fn end_procsubs(&mut self, mark: usize) {
        let done = self.procsubs.split_off(mark);
        for p in &done {
            sys::close(p.fd);
        }
        for p in done {
            if p.output {
                self.wait_for(p.pid);
            } else {
                self.procsub_orphans.push(p.pid);
            }
        }
        self.reap_procsub_orphans();
    }

    /// Closes our ends of the pipes after the first `mark`, without waiting
    /// (`exec 3< <(cmd)` has made copies).
    pub fn detach_procsubs(&mut self, mark: usize) {
        for p in self.procsubs.split_off(mark) {
            sys::close(p.fd);
            self.procsub_orphans.push(p.pid);
        }
    }

    fn reap_procsub_orphans(&mut self) {
        self.procsub_orphans
            .retain(|&pid| matches!(sys::waitpid(pid, libc::WNOHANG), Ok(None)));
    }
}
