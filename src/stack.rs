//! A guard against running out of stack.
//!
//! Recursion in a script (functions, `eval`, `.`, traps) and deeply nested
//! input recurse in the executor and the parser. Without a guard they end in
//! SIGSEGV, which in an interactive shell closes the terminal: `main` is a C
//! `main`, without the handler that Rust's start-up installs to report
//! overflows. Functions also have a depth limit of their own, as in dash
//! (`MAX_FUNC_DEPTH`); this catches the rest.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The address of a local of `main`: the stack's base, near enough.
static BASE: AtomicUsize = AtomicUsize::new(0);

/// How many shell functions can be running at once: the next call is an
/// error, as in Debian's dash (its patch for Debian bug 579815).
pub const MAX_FUNC_DEPTH: usize = 1000;

/// The error when the guard stops something.
pub const TOO_DEEP: &str = "nested too deeply";

/// Stack use below this is never checked against the limit, so that shells
/// that don't nest deeply make no system call for it.
const UNCHECKED: usize = 1 << 20;

/// Kept free below the guard, for the work between two checks.
const MARGIN: usize = 256 << 10;

/// The limit when the stack's size is unlimited.
const UNLIMITED: usize = 1 << 30;

/// Records the stack's base. Called first thing in `main` (so never in unit
/// tests, where `ok()` is always true).
#[cfg_attr(test, allow(dead_code))]
pub fn init() {
    let marker = 0u8;
    BASE.store(std::ptr::addr_of!(marker) as usize, Ordering::Relaxed);
}

/// Whether there is stack left for another level of nesting. The stack
/// grows down on the architectures luish supports.
#[inline]
pub fn ok() -> bool {
    let marker = 0u8;
    let here = std::ptr::addr_of!(marker) as usize;
    let used = BASE.load(Ordering::Relaxed).saturating_sub(here);
    used < UNCHECKED || used + MARGIN < limit()
}

/// The stack's size limit (`ulimit -s`, which can change while the shell
/// runs, so it is read each time).
#[cold]
fn limit() -> usize {
    // SAFETY: getrlimit only writes the struct it is given.
    let mut rl: libc::rlimit = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrlimit(libc::RLIMIT_STACK, &mut rl) } != 0 || rl.rlim_cur == libc::RLIM_INFINITY {
        return UNLIMITED;
    }
    (rl.rlim_cur as usize).min(UNLIMITED)
}
