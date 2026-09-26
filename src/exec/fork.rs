//! Forking, the child-side reset, and waiting for children.

use crate::shell::{ExecResult, Flow, Shell};
use crate::signals::{self, Disposition, NSIG};
use crate::sys;

impl Shell {
    /// Forks. In the child, resets the state that a subshell must not
    /// inherit and returns 0; in the parent, returns the child's pid.
    pub fn fork_or_error(&mut self) -> Result<i32, Flow> {
        match sys::fork() {
            Ok(0) => {
                self.child_reset();
                Ok(0)
            }
            Ok(pid) => Ok(pid),
            Err(e) => {
                self.error(format!("Cannot fork: {}", sys::strerror(e)));
                Err(Flow::Error(2))
            }
        }
    }

    fn child_reset(&mut self) {
        self.in_subshell = true;
        self.jobs.clear();
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
            for sig in [
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTERM,
                libc::SIGTSTP,
                libc::SIGTTIN,
                libc::SIGTTOU,
            ] {
                if !self.ignored_on_entry[sig as usize] && self.traps[sig as usize].is_none() {
                    signals::set_disposition(sig, Disposition::Default);
                }
            }
            signals::clear_pending(libc::SIGINT as usize);
        }
        self.interactive = false;
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
                    // Already reaped (e.g. by `jobs.reap()`).
                    if let Some(i) = self.jobs.find_pid(pid) {
                        return self.jobs.jobs[i].status().unwrap_or(127);
                    }
                    return 127;
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
