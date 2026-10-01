//! The right prompt (zsh's `RPROMPT`), at the right edge of the prompt's
//! last row, shown while the line (and its autosuggestion) leaves a column
//! free before it, as in zsh.
//!
//! rustyline has no right prompt, so the highlighter writes it after the
//! line, between a save and a restore of the cursor (`ESC 7`, `ESC 8`), so
//! that the cursor is left where rustyline expects it. rustyline clears and
//! redraws the line's rows on every change, so the right prompt is erased
//! with them and drawn again only if there is still room. That needs every
//! change to redraw, even a key typed at the end of the line, which
//! rustyline otherwise writes alone: `highlight_char` asks for it.

use std::cell::Cell;
use std::fmt::Write;

use super::menu;
use crate::prompt::Prompt;
use crate::sys;

#[derive(Default)]
pub struct Right {
    /// What to write, or empty for no right prompt.
    text: String,
    /// The columns it takes.
    width: usize,
    /// The columns to leave free at the right edge (`ZLE_RPROMPT_INDENT`).
    indent: usize,
    /// Not drawn once the line is accepted (`prompt.transient_rprompt`).
    transient: bool,
    /// The columns of the autosuggestion, which the hinter notes before the
    /// line is drawn.
    pub hint: Cell<usize>,
    /// Whether the line is being drawn as it was accepted.
    pub accepting: Cell<bool>,
}

impl Right {
    /// Sets the right prompt for the next line. One that takes more than a
    /// row is left out, as in zsh.
    pub fn set(&mut self, prompt: Option<&Prompt>, indent: usize, transient: bool) {
        self.text.clear();
        self.width = 0;
        if let Some(p) = prompt {
            let plain = String::from_utf8_lossy(p.plain.as_ref().unwrap_or(&p.text));
            if let (0, width) = menu::position(&plain, usize::MAX) {
                self.text = String::from_utf8_lossy(&p.text).into_owned();
                self.width = width;
            }
        }
        self.indent = indent;
        self.transient = transient;
        self.hint.set(0);
        self.accepting.set(false);
    }

    pub fn is_set(&self) -> bool {
        !self.text.is_empty()
    }

    /// What to write after `line`, which follows `prompt` (as the line
    /// editor measures it), to draw the right prompt, if it fits.
    pub fn draw(&self, prompt: &str, line: &str) -> Option<String> {
        if !self.is_set() || self.transient && self.accepting.get() {
            return None;
        }
        let cols = sys::window_size(1).map_or(80, |(c, _)| if c == 0 { 80 } else { c });
        let start = cols.checked_sub(self.indent + self.width)?;
        let (row, _) = menu::position(prompt, cols);
        let (end_row, end) = menu::position(&[prompt, line].concat(), cols);
        if end_row != row || end + self.hint.get() >= start {
            return None;
        }
        let mut s = String::with_capacity(self.text.len() + 12);
        let _ = write!(s, "\x1b7\x1b[{}G{}\x1b8", start + 1, self.text);
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn right(text: &str, plain: Option<&str>) -> Right {
        let p = Prompt {
            text: text.as_bytes().to_vec(),
            plain: plain.map(|p| p.as_bytes().to_vec()),
        };
        let mut r = Right::default();
        r.set(Some(&p), 1, false);
        r
    }

    #[test]
    fn measured_without_escapes() {
        let r = right("\x1b[31mabc\x1b[39m", Some("abc"));
        assert_eq!(r.width, 3);
        assert!(r.is_set());
        // Raw escape sequences, without %{...%}, are skipped too.
        assert_eq!(right("\x1b[31mab", None).width, 2);
    }

    #[test]
    fn more_than_a_row() {
        assert!(!right("a\nb", None).is_set());
        let mut r = Right::default();
        r.set(None, 1, false);
        assert!(!r.is_set());
    }
}
