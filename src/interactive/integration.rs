//! Telling the terminal where the prompts, the commands and their output
//! are (OSC 133, semantic prompt marks), and the current directory (OSC 7),
//! so that it can jump between prompts, select a command's output, and open
//! new windows in the same directory; and in vi mode, setting the cursor's
//! shape by the input mode (DECSCUSR). Only in the line editor, unless
//! `terminal.no_integration` is set.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use rustyline::{InputMode, KeyCode, KeyEvent, Modifiers};

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

/// Whether the cursor's shape follows the vi input mode, while the editor
/// reads a line.
static VI_CURSOR: AtomicBool = AtomicBool::new(false);
/// The shape the cursor was last given (a DECSCUSR parameter), 0 for the
/// terminal's default.
static SHAPE: AtomicU8 = AtomicU8::new(0);

/// The cursor's shape for an input mode: a bar to insert, an underline to
/// replace, a block for commands, as fish has them.
fn shape(mode: InputMode) -> u8 {
    match mode {
        InputMode::Insert => 6,
        InputMode::Replace => 4,
        InputMode::Command => 2,
    }
}

fn set_shape(shape: u8) {
    if SHAPE.swap(shape, Ordering::Relaxed) != shape {
        sys::write_all(1, format!("\x1b[{shape} q").as_bytes());
    }
}

/// Before the editor reads a line, if the cursor's shape is to follow the
/// vi mode (`vi`): it starts inserting.
pub fn line_starts(vi: bool) {
    VI_CURSOR.store(vi, Ordering::Relaxed);
    if vi {
        set_shape(shape(InputMode::Insert));
    }
}

/// After the editor read a line: the terminal's own cursor for the
/// commands that run.
pub fn line_done() {
    VI_CURSOR.store(false, Ordering::Relaxed);
    set_shape(0);
}

/// Before the editor acts on `key` in vi mode, in `mode`: the cursor's
/// shape for the mode the key leads to. The editor tells only the mode
/// before each key, so the shape is right again at the next key if this
/// guessed wrong.
pub fn vi_key(key: KeyEvent, mode: InputMode) {
    if VI_CURSOR.load(Ordering::Relaxed) {
        set_shape(shape(next_mode(mode, key)));
    }
}

/// The input mode after `key` in `mode`, as rustyline's vi keys change it.
fn next_mode(mode: InputMode, key: KeyEvent) -> InputMode {
    match (mode, key) {
        (InputMode::Command, KeyEvent(KeyCode::Char(c), Modifiers::NONE)) => match c {
            'a' | 'A' | 'c' | 'C' | 'i' | 'I' | 's' | 'S' => InputMode::Insert,
            'R' => InputMode::Replace,
            _ => InputMode::Command,
        },
        (InputMode::Command, _) => InputMode::Command,
        (_, KeyEvent(KeyCode::Esc, _)) => InputMode::Command,
        // Alt and a key: that key as a command.
        (_, KeyEvent(KeyCode::Char(c), Modifiers::ALT)) => {
            next_mode(InputMode::Command, KeyEvent(KeyCode::Char(c), Modifiers::NONE))
        }
        _ => mode,
    }
}

/// The `file:` URL of `path` on `host`, with the bytes that aren't
/// unreserved in a URL path percent-encoded.
pub fn file_url(host: &[u8], path: &[u8]) -> Vec<u8> {
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
    fn vi_modes() {
        // As the cursor's shapes, since `InputMode` isn't `Debug`.
        let (cmd, ins, rep) = (InputMode::Command, InputMode::Insert, InputMode::Replace);
        let (bar, underline, block) = (6, 4, 2);
        let next = |mode, key| shape(next_mode(mode, key));
        let key = |c| KeyEvent(KeyCode::Char(c), Modifiers::NONE);
        let esc = KeyEvent(KeyCode::Esc, Modifiers::NONE);
        let alt = |c| KeyEvent(KeyCode::Char(c), Modifiers::ALT);
        assert_eq!(next(cmd, key('i')), bar);
        assert_eq!(next(cmd, key('c')), bar);
        assert_eq!(next(cmd, key('R')), underline);
        assert_eq!(next(cmd, key('w')), block);
        assert_eq!(next(cmd, esc), block);
        assert_eq!(next(ins, key('w')), bar);
        assert_eq!(next(ins, esc), block);
        assert_eq!(next(rep, key('w')), underline);
        assert_eq!(next(rep, esc), block);
        assert_eq!(next(ins, alt('b')), block);
        assert_eq!(next(ins, alt('A')), bar);
    }

    #[test]
    fn prompts() {
        assert_eq!(mark_prompt("$ ", false), "\x1b]133;A\x07$ \x1b]133;B\x07");
        assert_eq!(mark_prompt("> ", true), "\x1b]133;A;k=s\x07> \x1b]133;B\x07");
    }
}
