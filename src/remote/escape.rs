//! The client's escapes, as ssh's: the escape character (`~`) at the start
//! of a line, then a key. ssh's own can't be used, as its input is the
//! protocol (and `ssh -T` turns them off), so the client sees the keys
//! first. While a command runs, `Escapes` filters what is typed as ssh
//! does; in the line editor, `interactive::remote::escape_key` does the
//! same at the start of an empty line.

/// What an escape does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// `~.`: closes the connection.
    Disconnect,
    /// `~^Z`: suspends the client.
    Suspend,
    /// `~?`: lists the escapes.
    Help,
}

/// The escape that `key` makes after the escape character. None for the
/// others, which are passed on after the escape character (as ssh does
/// with its own that luish doesn't have, such as `~C`), apart from the
/// escape character itself, which is passed on once.
pub fn action(key: u8) -> Option<Action> {
    match key {
        b'.' => Some(Action::Disconnect),
        0x1a => Some(Action::Suspend),
        b'?' => Some(Action::Help),
        _ => None,
    }
}

/// What the terminal shows for the escape, as ssh: the rest of it, after
/// the escape character (followed by a newline).
pub fn echo(a: Action) -> &'static [u8] {
    match a {
        Action::Disconnect => b".",
        Action::Suspend => b"^Z [suspend luish]",
        Action::Help => b"?",
    }
}

/// `~?`'s list, for the terminal in raw mode.
pub fn help(ch: u8) -> Vec<u8> {
    let c = ch as char;
    format!(
        "Supported escape sequences:\r\n \
         {c}.   - terminate connection\r\n \
         {c}^Z  - suspend luish\r\n \
         {c}?   - this message\r\n \
         {c}{c}   - send the escape character by typing it twice\r\n\
         (Note that escapes are only recognized immediately after newline \
         or at the start of an empty line.)\r\n"
    )
    .into_bytes()
}

/// The escapes in the keys typed while a command runs.
pub struct Escapes {
    /// The escape character (None for `-o ssh.escape_char=none`).
    ch: Option<u8>,
    /// Whether the last key passed on ended a line (as at the start).
    at_start: bool,
    /// Whether the escape character was typed, and is held until the next
    /// key says what it is.
    held: bool,
}

impl Escapes {
    pub fn new(ch: Option<u8>) -> Escapes {
        Escapes {
            ch,
            at_start: true,
            held: false,
        }
    }

    /// Takes `key`: adds what it passes on to `out`, or returns the escape
    /// it ends.
    pub fn key(&mut self, key: u8, out: &mut Vec<u8>) -> Option<Action> {
        let Some(ch) = self.ch else {
            out.push(key);
            return None;
        };
        if std::mem::take(&mut self.held) {
            if let Some(a) = action(key) {
                // Still at the start of a line, as ssh.
                return Some(a);
            }
            if key != ch {
                out.push(ch);
            }
        } else if self.at_start && key == ch {
            self.held = true;
            return None;
        }
        self.at_start = key == b'\r' || key == b'\n';
        out.push(key);
        None
    }

    /// A line was read: the keys after it start a line. True if the escape
    /// character was held, which the line editor then starts with.
    pub fn line_read(&mut self) -> bool {
        self.at_start = true;
        std::mem::take(&mut self.held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys passed on, and the escapes, for `keys` typed in one go.
    fn run(e: &mut Escapes, keys: &[u8]) -> (Vec<u8>, Vec<Action>) {
        let mut out = Vec::new();
        let actions = keys.iter().filter_map(|&k| e.key(k, &mut out)).collect();
        (out, actions)
    }

    #[test]
    fn escapes() {
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(run(&mut e, b"~."), (vec![], vec![Action::Disconnect]));
        let mut e = Escapes::new(Some(b'~'));
        // Not after other keys; after a newline (or a carriage return).
        assert_eq!(
            run(&mut e, b"a~.\r~?\n~\x1a"),
            (b"a~.\r\n".to_vec(), vec![Action::Help, Action::Suspend])
        );
        // After an escape, still at the start of a line.
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(run(&mut e, b"~?~."), (vec![], vec![Action::Help, Action::Disconnect]));
        // Twice sends it once, and then it isn't at the start of a line.
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(run(&mut e, b"~~."), (b"~.".to_vec(), vec![]));
        // Any other key: both, as `~/` (and ssh's escapes that luish lacks).
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(
            run(&mut e, b"~/x\r~C\r~\r~."),
            (b"~/x\r~C\r~\r".to_vec(), vec![Action::Disconnect])
        );
        // Held across reads.
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(run(&mut e, b"~"), (vec![], vec![]));
        assert_eq!(run(&mut e, b"."), (vec![], vec![Action::Disconnect]));
    }

    #[test]
    fn other_characters() {
        let mut e = Escapes::new(Some(b'%'));
        assert_eq!(
            run(&mut e, b"~.\r%%%.\r%."),
            (b"~.\r%%.\r".to_vec(), vec![Action::Disconnect])
        );
        let mut e = Escapes::new(None);
        assert_eq!(run(&mut e, b"~.\r~?"), (b"~.\r~?".to_vec(), vec![]));
    }

    #[test]
    fn line_read() {
        let mut e = Escapes::new(Some(b'~'));
        assert_eq!(run(&mut e, b"ls"), (b"ls".to_vec(), vec![]));
        assert!(!e.line_read());
        assert_eq!(run(&mut e, b"~"), (vec![], vec![]));
        assert!(e.line_read());
        assert_eq!(run(&mut e, b"."), (b".".to_vec(), vec![]));
    }
}
