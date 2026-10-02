//! Telling the terminal where the prompts, the commands and their output
//! are (OSC 133, semantic prompt marks), and the current directory (OSC 7),
//! so that it can jump between prompts, select a command's output, and open
//! new windows in the same directory. Only in the line editor, unless
//! `terminal.no_integration` is set.

use std::cell::{Cell, RefCell};

use crate::options::Opt;
use crate::shell::Shell;
use crate::sys;

thread_local! {
    /// Whether the start of a command's output was marked, so that its
    /// end is marked too.
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    /// The directory last reported to the terminal.
    static DIR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Whether to mark: the option is off and the editor's output, fd 1, is
/// the terminal (the caller checks that the editor supports it).
pub fn on(sh: &Shell) -> bool {
    !sh.opt(Opt::NoTermIntegration) && sys::isatty(1)
}

/// The prompt with its marks: where it starts (`k=s` for a continuation
/// line's) and where the command line starts.
pub fn mark_prompt(prompt: &str, continuation: bool) -> String {
    let kind = if continuation { ";k=s" } else { "" };
    format!("\x1b]133;A{kind}\x07{prompt}\x1b]133;B\x07")
}

/// Before a primary prompt: reports the current directory if it changed.
pub fn before_prompt(sh: &Shell) {
    let Some(dir) = sh.curdir.as_deref() else {
        return;
    };
    if DIR.with_borrow(|d| d == dir) {
        return;
    }
    DIR.set(dir.to_vec());
    let mut seq = b"\x1b]7;".to_vec();
    seq.extend_from_slice(&file_url(&sys::hostname(), dir));
    seq.push(0x07);
    sys::write_all(1, &seq);
}

/// Marks the start of a command's output, after its line was read.
pub fn command_starts(sh: &Shell) {
    if on(sh) {
        sys::write_all(1, b"\x1b]133;C\x07");
        RUNNING.set(true);
    }
}

/// Marks the end of a command's output, with its status.
pub fn command_done(status: i32) {
    if RUNNING.take() {
        sys::write_all(1, format!("\x1b]133;D;{status}\x07").as_bytes());
    }
}

/// The `file:` URL of `path` on `host`, with the bytes that aren't
/// unreserved in a URL path percent-encoded.
fn file_url(host: &[u8], path: &[u8]) -> Vec<u8> {
    let mut url = b"file://".to_vec();
    url.extend_from_slice(host);
    for &c in path {
        if c.is_ascii_alphanumeric() || b"/-._~".contains(&c) {
            url.push(c);
        } else {
            url.extend_from_slice(format!("%{c:02X}").as_bytes());
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(file_url(b"box", b"/home/me"), b"file://box/home/me");
        assert_eq!(file_url(b"box", b"/a b/50%/#?"), b"file://box/a%20b/50%25/%23%3F");
        assert_eq!(file_url(b"", "/é\n".as_bytes()), b"file:///%C3%A9%0A");
    }

    #[test]
    fn prompts() {
        assert_eq!(mark_prompt("$ ", false), "\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(mark_prompt("> ", true), "\x1b]133;A;k=s\x07> \x1b]133;B\x07");
    }
}
