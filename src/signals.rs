//! Signal dispositions, pending-signal flags, and signal names.

use std::sync::atomic::{AtomicBool, Ordering};

pub const NSIG: usize = 65;

static PENDING: [AtomicBool; NSIG] = [const { AtomicBool::new(false) }; NSIG];
static ANY_PENDING: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    if (sig as usize) < NSIG {
        PENDING[sig as usize].store(true, Ordering::SeqCst);
        ANY_PENDING.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Disposition {
    Default,
    Ignore,
    /// Record the signal in the pending flags.
    Catch,
}

pub fn set_disposition(sig: i32, d: Disposition) {
    // SAFETY: plain sigaction with a handler that only touches atomics.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = match d {
            Disposition::Default => libc::SIG_DFL,
            Disposition::Ignore => libc::SIG_IGN,
            Disposition::Catch => on_signal as *const () as libc::sighandler_t,
        };
        libc::sigemptyset(&mut sa.sa_mask);
        // No SA_RESTART: a trapped signal must interrupt `wait` and `read`.
        sa.sa_flags = 0;
        libc::sigaction(sig, &sa, std::ptr::null_mut());
    }
}

/// Signals that were ignored when the shell started (these can't be trapped).
pub fn ignored_on_entry() -> [bool; NSIG] {
    let mut out = [false; NSIG];
    for (sig, slot) in out.iter_mut().enumerate().skip(1) {
        // SAFETY: querying the current action only.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig as i32, std::ptr::null(), &mut old) == 0 {
                *slot = old.sa_sigaction == libc::SIG_IGN;
            }
        }
    }
    // Rust ignores SIGPIPE before main() runs, so we can't tell whether it
    // was ignored on entry. Assume it wasn't.
    out[libc::SIGPIPE as usize] = false;
    out
}

/// Takes the set of signals that arrived since the last call.
pub fn take_pending() -> Vec<usize> {
    if !ANY_PENDING.swap(false, Ordering::SeqCst) {
        return Vec::new();
    }
    (1..NSIG)
        .filter(|&s| PENDING[s].swap(false, Ordering::SeqCst))
        .collect()
}

/// The signals that have arrived, without clearing them.
pub fn peek_pending() -> Vec<usize> {
    (1..NSIG).filter(|&s| PENDING[s].load(Ordering::SeqCst)).collect()
}

pub fn any_pending() -> bool {
    ANY_PENDING.load(Ordering::SeqCst)
}

pub fn clear_pending(sig: usize) {
    PENDING[sig].store(false, Ordering::SeqCst);
}

const NAMES: &[(i32, &str)] = &[
    (libc::SIGHUP, "HUP"),
    (libc::SIGINT, "INT"),
    (libc::SIGQUIT, "QUIT"),
    (libc::SIGILL, "ILL"),
    (libc::SIGTRAP, "TRAP"),
    (libc::SIGABRT, "ABRT"),
    (libc::SIGBUS, "BUS"),
    (libc::SIGFPE, "FPE"),
    (libc::SIGKILL, "KILL"),
    (libc::SIGUSR1, "USR1"),
    (libc::SIGSEGV, "SEGV"),
    (libc::SIGUSR2, "USR2"),
    (libc::SIGPIPE, "PIPE"),
    (libc::SIGALRM, "ALRM"),
    (libc::SIGTERM, "TERM"),
    (libc::SIGSTKFLT, "STKFLT"),
    (libc::SIGCHLD, "CHLD"),
    (libc::SIGCONT, "CONT"),
    (libc::SIGSTOP, "STOP"),
    (libc::SIGTSTP, "TSTP"),
    (libc::SIGTTIN, "TTIN"),
    (libc::SIGTTOU, "TTOU"),
    (libc::SIGURG, "URG"),
    (libc::SIGXCPU, "XCPU"),
    (libc::SIGXFSZ, "XFSZ"),
    (libc::SIGVTALRM, "VTALRM"),
    (libc::SIGPROF, "PROF"),
    (libc::SIGWINCH, "WINCH"),
    (libc::SIGIO, "IO"),
    (libc::SIGPWR, "PWR"),
    (libc::SIGSYS, "SYS"),
];

pub fn name(sig: i32) -> String {
    if sig == 0 {
        return "EXIT".into();
    }
    if let Some((_, n)) = NAMES.iter().find(|(s, _)| *s == sig) {
        return (*n).into();
    }
    let rtmin = libc::SIGRTMIN();
    let rtmax = libc::SIGRTMAX();
    if sig == rtmin {
        "RTMIN".into()
    } else if sig == rtmax {
        "RTMAX".into()
    } else if sig > rtmin && sig < rtmax {
        format!("RTMIN+{}", sig - rtmin)
    } else {
        sig.to_string()
    }
}

/// Parses a signal name (with or without `SIG`, any case) or number.
pub fn parse(s: &[u8]) -> Option<i32> {
    let s = std::str::from_utf8(s).ok()?;
    if let Ok(n) = s.parse::<i32>() {
        return (0..NSIG as i32).contains(&n).then_some(n);
    }
    let up = s.to_ascii_uppercase();
    let up = up.strip_prefix("SIG").unwrap_or(&up);
    if up == "EXIT" {
        return Some(0);
    }
    if let Some((sig, _)) = NAMES.iter().find(|(_, n)| *n == up) {
        return Some(*sig);
    }
    let rtmin = libc::SIGRTMIN();
    let rtmax = libc::SIGRTMAX();
    match up {
        "RTMIN" => Some(rtmin),
        "RTMAX" => Some(rtmax),
        _ => {
            let n: i32 = up.strip_prefix("RTMIN+")?.parse().ok()?;
            (rtmin + n <= rtmax).then_some(rtmin + n)
        }
    }
}

/// Numbers of all real signals, for `kill -l` and `trap`.
pub fn all() -> impl Iterator<Item = i32> {
    1..=libc::SIGRTMAX()
}
