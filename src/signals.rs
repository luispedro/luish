//! Signal dispositions, pending-signal flags, and signal names.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
    // Remember whether it was ignored before the shell first changes it.
    ignored_on_entry(sig as usize);
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

/// Whether each signal was ignored when the shell started: 0 if not known
/// yet, 1 if not, 2 if it was.
static ENTRY: [AtomicU8; NSIG] = [const { AtomicU8::new(0) }; NSIG];

/// Whether `sig` was ignored when the shell started (then it can't be
/// trapped). As in dash, it is looked up when first needed, which is before
/// the shell first changes it (see `set_disposition`), rather than for every
/// signal at startup.
pub fn ignored_on_entry(sig: usize) -> bool {
    if sig == 0 || sig >= NSIG {
        return false;
    }
    match ENTRY[sig].load(Ordering::Relaxed) {
        0 => {
            // SAFETY: querying the current action only.
            let ignored = unsafe {
                let mut old: libc::sigaction = std::mem::zeroed();
                libc::sigaction(sig as i32, std::ptr::null(), &mut old) == 0 && old.sa_sigaction == libc::SIG_IGN
            };
            ENTRY[sig].store(if ignored { 2 } else { 1 }, Ordering::Relaxed);
            ignored
        }
        v => v == 2,
    }
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

/// The names of the usual signals, for completion.
pub fn names() -> impl Iterator<Item = &'static str> {
    NAMES.iter().map(|&(_, n)| n)
}

/// The name of a signal as dash's `signal_names` has it (`EXIT` for 0, a
/// number for a signal without a name).
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
    } else if sig > rtmin && sig - rtmin <= (rtmax - rtmin) / 2 {
        format!("RTMIN+{}", sig - rtmin)
    } else if sig > rtmin && sig < rtmax {
        format!("RTMAX-{}", rtmax - sig)
    } else {
        sig.to_string()
    }
}

/// dash's `decode_signal`: a number (digits only) below `NSIG`, or a
/// signal name in any case, without `SIG`, from signal `minsig` on.
pub fn parse(s: &[u8], minsig: i32) -> Option<i32> {
    if !s.is_empty() && s.iter().all(|c| c.is_ascii_digit()) {
        let n: i64 = std::str::from_utf8(s).ok()?.parse().unwrap_or(i64::MAX);
        return (n < NSIG as i64).then_some(n as i32);
    }
    (minsig..NSIG as i32).find(|&sig| name(sig).as_bytes().eq_ignore_ascii_case(s))
}

/// Whether `sig` has arrived and not yet been handled.
#[cfg(feature = "plugins")]
pub fn is_pending(sig: i32) -> bool {
    PENDING[sig as usize].load(Ordering::SeqCst)
}
