//! Forking, the child-side reset, and waiting for children.

use crate::jobs::Job;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals::{self, Disposition, NSIG};
use crate::sys;

/// How a forked child relates to job control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ForkKind {
    /// A process of a foreground job. The number is the job's process
    /// group, or 0 for the job's first process, which starts the group.
    Foreground(i32),
    /// A process of a background job; the number is as for `Foreground`.
    Background(i32),
    /// A child that is not a job (command substitution).
    NoJob,
}

impl Shell {
    /// Forks a child that is not a job.
    pub fn fork_or_error(&mut self) -> Result<i32, Flow> {
        self.fork_child(ForkKind::NoJob)
    }

    /// Forks. In the child, resets the state that a subshell must not
    /// inherit, sets up job control, and returns 0; in the parent, returns
    /// the child's pid.
    pub fn fork_child(&mut self, kind: ForkKind) -> Result<i32, Flow> {
        if matches!(
            kind,
            ForkKind::Foreground(0) | ForkKind::Background(0) | ForkKind::NoJob
        ) {
            // A new job: dash frees a finished job's slot at this point.
            self.jobs.reclaim(self.jobctl());
        }
        // A signal that arrives before the child has reset the dispositions
        // it inherited would be lost (ignored, or recorded for a trap the
        // child then clears), so keep signals blocked until then. Without
        // traps, job control or an interactive shell, the child inherits the
        // default dispositions and there is nothing to protect.
        let saved_mask = (self.interactive || self.jobctl() || self.has_traps()).then(sys::block_signals);
        match sys::fork() {
            Ok(0) => {
                let jobctl = self.jobctl.take();
                self.child_reset();
                match (kind, &jobctl) {
                    (ForkKind::Foreground(pgid) | ForkKind::Background(pgid), Some(t)) => {
                        let pgid = if pgid == 0 { sys::getpid() } else { pgid };
                        // This can fail because the parent does it too.
                        let _ = sys::setpgid(0, pgid);
                        if matches!(kind, ForkKind::Foreground(_)) {
                            let _ = sys::tcsetpgrp(t.fd, pgid);
                        }
                        for sig in [libc::SIGTSTP, libc::SIGTTOU] {
                            self.apply_trap_disposition(sig as usize);
                        }
                    }
                    (ForkKind::Background(pgid), None) => {
                        for sig in [libc::SIGINT, libc::SIGQUIT] {
                            if self.traps[sig as usize].is_none() {
                                signals::set_disposition(sig, Disposition::Ignore);
                            }
                        }
                        if pgid == 0
                            && let Ok(fd) = sys::open(b"/dev/null", libc::O_RDONLY, 0)
                        {
                            let _ = sys::dup2(fd, 0);
                            sys::close(fd);
                        }
                    }
                    _ => {}
                }
                if let Some(mask) = &saved_mask {
                    sys::set_signal_mask(mask);
                }
                Ok(0)
            }
            Ok(pid) => {
                if let Some(mask) = &saved_mask {
                    sys::set_signal_mask(mask);
                }
                if let (ForkKind::Foreground(pgid) | ForkKind::Background(pgid), true) = (kind, self.jobctl()) {
                    // This can fail because the child does it too.
                    let _ = sys::setpgid(pid, if pgid == 0 { pid } else { pgid });
                }
                Ok(pid)
            }
            Err(e) => {
                if let Some(mask) = &saved_mask {
                    sys::set_signal_mask(mask);
                }
                self.error(format!("Cannot fork: {}", sys::strerror(e)));
                Err(Flow::Error(2))
            }
        }
    }

    fn child_reset(&mut self) {
        self.in_subshell = true;
        self.jobs.clear();
        self.vars.child_reset();
        // Traps with an action are reset; ignored signals stay ignored.
        for sig in 1..NSIG {
            if self.traps[sig].as_ref().is_some_and(|a| !a.is_empty()) {
                self.traps[sig] = None;
                signals::set_disposition(sig as i32, Disposition::Default);
                signals::clear_pending(sig);
            }
        }
        self.traps[0] = None;
        if self.interactive {
            for sig in [libc::SIGINT, libc::SIGQUIT, libc::SIGTERM] {
                if !signals::ignored_on_entry(sig as usize) && self.traps[sig as usize].is_none() {
                    signals::set_disposition(sig, Disposition::Default);
                }
            }
            signals::clear_pending(libc::SIGINT as usize);
        }
        self.interactive = false;
    }

    /// Whether any trap has an action (which rules out replacing the shell
    /// process with a command, since the trap could still be needed).
    pub fn has_traps(&self) -> bool {
        self.traps.iter().any(|t| t.as_ref().is_some_and(|a| !a.is_empty()))
    }

    /// Waits for the processes of a foreground job and returns the job's
    /// status. Under job control, the job is recorded (with the command
    /// text from `text`, one entry per process) so that it can be stopped.
    pub fn wait_foreground(&mut self, pids: &[i32], text: impl FnOnce() -> Vec<String>) -> i32 {
        // Nothing can change the option while the shell waits, so it is
        // as it was when the pipeline started, as POSIX requires.
        let pipefail = pids.len() > 1 && self.opt(Opt::Pipefail);
        if !self.jobctl() {
            let mut status = 0;
            for &pid in pids {
                let s = self.wait_for(pid);
                if s != 0 || !pipefail {
                    status = s;
                }
            }
            return status;
        }
        let procs = pids.iter().copied().zip(text()).collect();
        let i = self.jobs.add(Job::new(procs, true, pipefail), false);
        self.wait_job(i)
    }

    /// Exits a forked child with the status of `r`.
    pub fn child_exit(&mut self, r: ExecResult) -> ! {
        let status = match r {
            Ok(s) => s,
            Err(Flow::Exit(n) | Flow::Error(n) | Flow::Return(n)) => n,
            Err(Flow::Break(_) | Flow::Continue(_)) => self.last_status,
        };
        self.exit(status)
    }

    /// Waits for a child and returns its `$?`-style status.
    pub fn wait_for(&mut self, pid: i32) -> i32 {
        loop {
            match sys::waitpid(pid, 0) {
                Ok(Some((_, ws))) => {
                    report_signaled(&ws);
                    return ws.code();
                }
                Ok(None) => continue,
                Err(libc::EINTR) => continue,
                Err(_) => {
                    // Already reaped (e.g. by `wait`).
                    let status = self.jobs.find_pid(pid).and_then(|i| {
                        let p = self.jobs.get(i).procs.iter().find(|p| p.pid == pid)?;
                        p.status.map(|s| s.code())
                    });
                    return status.unwrap_or(127);
                }
            }
        }
    }
}

/// Prints a message for a child killed by a signal, as dash does (except
/// for SIGINT and SIGPIPE).
pub fn report_signaled(ws: &sys::WaitStatus) {
    if let sys::WaitStatus::Signaled(sig, core) = *ws
        && sig != libc::SIGINT
        && sig != libc::SIGPIPE
    {
        let mut msg = signal_description(sig);
        if core {
            msg.push_str(" (core dumped)");
        }
        msg.push('\n');
        sys::write_all(2, msg.as_bytes());
    }
}

pub fn signal_description(sig: i32) -> String {
    // SAFETY: strsignal returns a pointer to a (possibly static) string.
    unsafe {
        let p = libc::strsignal(sig);
        if p.is_null() {
            return format!("Signal {sig}");
        }
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}
