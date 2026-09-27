//! `trap` and signal dispositions.

use super::vars::single_quote;
use crate::shell::{ExecResult, Shell};
use crate::signals::{self, Disposition, NSIG};

impl Shell {
    /// Sets the disposition of `sig` to match its trap (or the shell's
    /// default handling when the trap is reset).
    pub fn apply_trap_disposition(&self, sig: usize) {
        if sig == 0 {
            return;
        }
        let d = match &self.traps[sig] {
            Some(a) if a.is_empty() => Disposition::Ignore,
            Some(_) => Disposition::Catch,
            None => self.default_disposition(sig as i32),
        };
        signals::set_disposition(sig as i32, d);
    }

    pub fn default_disposition(&self, sig: i32) -> Disposition {
        if self.interactive {
            if sig == libc::SIGINT {
                return Disposition::Catch;
            }
            if sig == libc::SIGQUIT || sig == libc::SIGTERM {
                return Disposition::Ignore;
            }
        }
        // Under job control the shell itself must not be stopped from the
        // terminal (as in dash, SIGTTIN keeps its default action).
        if self.jobctl() && matches!(sig, libc::SIGTSTP | libc::SIGTTOU) {
            return Disposition::Ignore;
        }
        Disposition::Default
    }
}

pub fn trap(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut args = &argv[1..];
    // As in dash (`nextopt("")`), trap has no options.
    match args.first() {
        Some(a) if a == b"--" => args = &args[1..],
        Some(a) if a.len() > 1 && a[0] == b'-' => {
            sh.berr(&argv[0], format!("Illegal option -{}", a[1] as char));
            return Err(crate::shell::Flow::Error(2));
        }
        _ => {}
    }
    if args.is_empty() {
        let mut out = Vec::new();
        for (sig, t) in sh.traps.iter().enumerate() {
            if let Some(action) = t {
                out.extend_from_slice(b"trap -- ");
                out.extend(single_quote(action));
                out.push(b' ');
                out.extend_from_slice(signals::name(sig as i32).as_bytes());
                out.push(b'\n');
            }
        }
        return Ok(sh.out_status(&out));
    }
    // `trap N...` with a signal number first resets (dash's `decode_signum`).
    let is_signum = |a: &[u8]| !a.is_empty() && a.iter().all(|c| c.is_ascii_digit()) && signals::parse(a, 0).is_some();
    let (action, sigs) = if args.len() == 1 || is_signum(&args[0]) {
        (None, args)
    } else if args[0] == b"-" {
        (None, &args[1..])
    } else {
        (Some(args[0].clone()), &args[1..])
    };
    let status = 0;
    for s in sigs {
        let Some(sig) = signals::parse(s, 0) else {
            // dash's wording, without the usual prefix; the rest are skipped.
            let mut msg = b"trap: ".to_vec();
            msg.extend_from_slice(s);
            msg.extend_from_slice(b": bad trap\n");
            crate::sys::write_all(2, &msg);
            return Ok(1);
        };
        let sig = sig as usize;
        if sig >= NSIG {
            continue;
        }
        if sig != 0 && crate::signals::ignored_on_entry(sig) && !sh.interactive {
            continue;
        }
        sh.traps[sig] = action.clone();
        sh.apply_trap_disposition(sig);
    }
    Ok(status)
}
