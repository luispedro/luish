//! Key bindings: luish's keymap over rustyline's, with zsh's widget names,
//! and the `bindkey` built-in that changes it.
//!
//! rustyline decodes the keys. Each key sequence in the keymap gets a
//! handler (`Dispatch`) that first lets the completion menu act on the key,
//! then runs the widget the keymap binds it to; a key luish doesn't bind
//! does what rustyline does. The keymap applies in emacs mode only (vi mode
//! keeps rustyline's keys).
//!
//! A widget that rustyline has is returned as its command. The others:
//!
//! - Word motions and kills stop where zsh's do, at characters that are
//!   neither alphanumeric nor in `$WORDCHARS`. The target is computed here
//!   and given to rustyline as a character search, so that kills go to the
//!   kill ring as usual.
//! - `history-beginning-search-backward` and `-forward` are rustyline's
//!   anchored search, which the history store (`history.rs`) makes behave
//!   as zsh's: an empty prefix matches every entry, entries equal to the
//!   line are skipped, and going forward past the newest match brings back
//!   the line as typed. The handler records the line in `Search` for this.
//! - `insert-last-word` needs the history, which only the completer and
//!   the hinter can see: the handler records the request, and returns
//!   `Cmd::Complete` so that the completer does it (`last_word`).
//! - `accept-line-and-down-history` accepts the line and records the
//!   position in the history (which the hinter notes at each redraw), so
//!   that the next prompt starts with the entry after it.
//!
//! The keymap is kept on the shell (`Keymap`, only the changes from the
//! defaults), so that `savestate` can save it; the line editor gets a copy
//! before each prompt when it has changed.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use rustyline::history::{History, SearchDirection};
use rustyline::{
    Anchor, Cmd, ConditionalEventHandler, EditMode, Editor, Event, EventContext, EventHandler, Helper, KeyCode,
    KeyEvent, Modifiers, Movement, RepeatCount, Word,
};

use super::history::Search;
use super::menu::{self, Menu};
use crate::options::Opt;
use crate::shell::{ExecResult, Shell};

/// zsh's default `WORDCHARS`: the characters other than alphanumerics that
/// are part of words.
const DEFAULT_WORDCHARS: &str = "*?_-.[]~=/&;!#$%^(){}<>";

/// What a widget does.
#[derive(Clone, Copy)]
enum Action {
    /// A rustyline command, given the repeat count.
    Cmd(fn(RepeatCount) -> Cmd),
    /// A move or kill by words, as zsh's.
    Word(WordOp),
    /// `forward-char` and `end-of-line`, which accept an autosuggestion at
    /// the end of the line (as zsh-autosuggestions binds them).
    AcceptHint(fn(RepeatCount) -> Cmd),
    HistorySearch(SearchDirection),
    InsertLastWord,
    AcceptLineAndDownHistory,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WordOp {
    Backward,
    Forward,
    BackwardKill,
    Kill,
}

/// The widgets, by zsh's names, sorted.
const WIDGETS: &[(&str, Action)] = &[
    ("accept-line", Action::Cmd(|_| Cmd::AcceptLine)),
    ("accept-line-and-down-history", Action::AcceptLineAndDownHistory),
    ("backward-char", Action::Cmd(|n| Cmd::Move(Movement::BackwardChar(n)))),
    (
        "backward-delete-char",
        Action::Cmd(|n| Cmd::Kill(Movement::BackwardChar(n))),
    ),
    (
        "backward-kill-line",
        Action::Cmd(|_| Cmd::Kill(Movement::BeginningOfLine)),
    ),
    ("backward-kill-word", Action::Word(WordOp::BackwardKill)),
    ("backward-word", Action::Word(WordOp::Backward)),
    (
        "beginning-of-buffer-or-history",
        Action::Cmd(|_| Cmd::BeginningOfHistory),
    ),
    ("beginning-of-history", Action::Cmd(|_| Cmd::BeginningOfHistory)),
    (
        "beginning-of-line",
        Action::Cmd(|_| Cmd::Move(Movement::BeginningOfLine)),
    ),
    ("capitalize-word", Action::Cmd(|_| Cmd::CapitalizeWord)),
    ("clear-screen", Action::Cmd(|_| Cmd::ClearScreen)),
    ("delete-char", Action::Cmd(|n| Cmd::Kill(Movement::ForwardChar(n)))),
    ("down-case-word", Action::Cmd(|_| Cmd::DowncaseWord)),
    ("down-history", Action::Cmd(|_| Cmd::NextHistory)),
    ("down-line-or-history", Action::Cmd(Cmd::LineDownOrNextHistory)),
    ("end-of-buffer-or-history", Action::Cmd(|_| Cmd::EndOfHistory)),
    ("end-of-history", Action::Cmd(|_| Cmd::EndOfHistory)),
    ("end-of-line", Action::AcceptHint(|_| Cmd::Move(Movement::EndOfLine))),
    ("expand-or-complete", Action::Cmd(|_| Cmd::Complete)),
    (
        "forward-char",
        Action::AcceptHint(|n| Cmd::Move(Movement::ForwardChar(n))),
    ),
    ("forward-word", Action::Word(WordOp::Forward)),
    (
        "history-beginning-search-backward",
        Action::HistorySearch(SearchDirection::Reverse),
    ),
    (
        "history-beginning-search-forward",
        Action::HistorySearch(SearchDirection::Forward),
    ),
    (
        "history-incremental-search-backward",
        Action::Cmd(|_| Cmd::ReverseSearchHistory),
    ),
    (
        "history-incremental-search-forward",
        Action::Cmd(|_| Cmd::ForwardSearchHistory),
    ),
    ("insert-last-word", Action::InsertLastWord),
    ("kill-buffer", Action::Cmd(|_| Cmd::Kill(Movement::WholeBuffer))),
    ("kill-line", Action::Cmd(|_| Cmd::Kill(Movement::EndOfLine))),
    ("kill-whole-line", Action::Cmd(|_| Cmd::Kill(Movement::WholeLine))),
    ("kill-word", Action::Word(WordOp::Kill)),
    ("quoted-insert", Action::Cmd(|_| Cmd::QuotedInsert)),
    ("redisplay", Action::Cmd(|_| Cmd::Repaint)),
    ("send-break", Action::Cmd(|_| Cmd::Abort)),
    ("transpose-chars", Action::Cmd(|_| Cmd::TransposeChars)),
    ("transpose-words", Action::Cmd(Cmd::TransposeWords)),
    ("undefined-key", Action::Cmd(|_| Cmd::Noop)),
    ("undo", Action::Cmd(Cmd::Undo)),
    ("up-case-word", Action::Cmd(|_| Cmd::UpcaseWord)),
    ("up-history", Action::Cmd(|_| Cmd::PreviousHistory)),
    ("up-line-or-history", Action::Cmd(Cmd::LineUpOrPreviousHistory)),
    ("yank", Action::Cmd(|n| Cmd::Yank(n, Anchor::Before))),
    ("yank-pop", Action::Cmd(|_| Cmd::YankPop)),
];

/// The default bindings of the emacs keymap: zsh's, except that Up and Down
/// search the history for the text before the cursor.
const DEFAULTS: &[(&str, &str)] = &[
    ("^A", "beginning-of-line"),
    ("^B", "backward-char"),
    ("^E", "end-of-line"),
    ("^F", "forward-char"),
    ("^K", "kill-line"),
    ("^L", "clear-screen"),
    ("^N", "down-line-or-history"),
    ("^O", "accept-line-and-down-history"),
    ("^P", "up-line-or-history"),
    ("^R", "history-incremental-search-backward"),
    ("^S", "history-incremental-search-forward"),
    ("^T", "transpose-chars"),
    ("^U", "kill-whole-line"),
    ("^V", "quoted-insert"),
    ("^W", "backward-kill-word"),
    ("^Y", "yank"),
    ("^_", "undo"),
    ("^?", "backward-delete-char"),
    ("^[.", "insert-last-word"),
    ("^[<", "beginning-of-buffer-or-history"),
    ("^[>", "end-of-buffer-or-history"),
    ("^[_", "insert-last-word"),
    ("^[b", "backward-word"),
    ("^[c", "capitalize-word"),
    ("^[d", "kill-word"),
    ("^[f", "forward-word"),
    ("^[l", "down-case-word"),
    ("^[t", "transpose-words"),
    ("^[u", "up-case-word"),
    ("^[y", "yank-pop"),
    ("^[^?", "backward-kill-word"),
    ("^[[A", "history-beginning-search-backward"),
    ("^[[B", "history-beginning-search-forward"),
    ("^[[C", "forward-char"),
    ("^[[D", "backward-char"),
    ("^[[H", "beginning-of-line"),
    ("^[[F", "end-of-line"),
    ("^[[3~", "delete-char"),
];

fn widget(name: &[u8]) -> Option<(&'static str, Action)> {
    WIDGETS.iter().find(|w| w.0.as_bytes() == name).copied()
}

type Keys = Vec<KeyEvent>;

/// The default bindings, decoded.
fn defaults() -> impl Iterator<Item = (Keys, &'static str)> {
    DEFAULTS
        .iter()
        .filter_map(|&(seq, w)| Some((decode(&parse(seq.as_bytes()).ok()?)?, w)))
}

fn default_for(keys: &[KeyEvent]) -> Option<&'static str> {
    defaults().find(|(k, _)| k == keys).map(|(_, w)| w)
}

/// The shell's changes to the default keymap: a sequence bound to a widget,
/// or (None) given back to rustyline.
#[derive(Default, Clone)]
pub struct Keymap {
    changes: Vec<(Keys, Option<&'static str>)>,
    /// Counts the changes, so that the line editor copies the keymap only
    /// when it has changed.
    version: u64,
}

impl Keymap {
    fn set(&mut self, keys: Keys, widget: Option<&'static str>) {
        self.version += 1;
        self.changes.retain(|c| c.0 != keys);
        let default = default_for(&keys);
        if widget != default {
            self.changes.push((keys, widget));
        }
    }

    /// The bindings in effect, sorted by how their sequences are written.
    fn bindings(&self) -> Vec<(String, &'static str)> {
        let mut map: HashMap<Keys, &'static str> = defaults().collect();
        for (k, w) in &self.changes {
            match w {
                Some(w) => map.insert(k.clone(), w),
                None => map.remove(k),
            };
        }
        // In the order of the bytes the terminal sends, as zsh lists them.
        let mut out: Vec<_> = map.into_iter().map(|(k, w)| (show(&k), w)).collect();
        out.sort_by_cached_key(|(s, _)| parse(s.as_bytes()).unwrap_or_default());
        out
    }

    /// The changes, with each sequence as `bindkey` shows it, and the
    /// version (for the SSH mode's client).
    pub fn to_wire(&self) -> (u64, Vec<(String, Option<&'static str>)>) {
        (self.version, self.changes.iter().map(|(k, w)| (show(k), *w)).collect())
    }

    /// The keymap that `to_wire` gave. Sequences or widgets that this luish
    /// doesn't know are left out.
    pub fn from_wire(version: u64, changes: Vec<(String, Option<String>)>) -> Keymap {
        let changes = (changes.into_iter())
            .filter_map(|(seq, w)| {
                let keys = decode(&parse(seq.as_bytes()).ok()?)?;
                let w = match w {
                    Some(w) => Some(widget(w.as_bytes())?.0),
                    None => None,
                };
                Some((keys, w))
            })
            .collect();
        Keymap { changes, version }
    }

    /// Commands that restore the changes (for `savestate`): the name of
    /// each, and the command.
    pub fn state(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out: Vec<_> = (self.changes.iter())
            .map(|(k, w)| {
                let seq = show(k);
                let cmd = match w {
                    Some(w) => format!("__luish_internal bindkey {} {w}\n", quote(&seq)),
                    None => format!("__luish_internal bindkey -r {}\n", quote(&seq)),
                };
                (seq.into_bytes(), cmd.into_bytes())
            })
            .collect();
        out.sort();
        out
    }
}

/// The command that gives the key sequence `seq` (as `show` writes it) its
/// default binding back (for the startup cache's differences).
pub fn restore_command(seq: &[u8]) -> Vec<u8> {
    let keys = parse(seq).ok().and_then(|b| decode(&b));
    let quoted = quote(&String::from_utf8_lossy(seq));
    match keys.as_deref().and_then(default_for) {
        Some(w) => format!("__luish_internal bindkey {quoted} {w}\n"),
        None => format!("__luish_internal bindkey -r {quoted}\n"),
    }
    .into_bytes()
}

fn quote(s: &str) -> String {
    String::from_utf8_lossy(&crate::builtins::single_quote(s.as_bytes())).into_owned()
}

/// Parses a key sequence as zsh's `bindkey` takes it: `^X` for a control
/// character (`^?` is DEL), and backslash escapes (`\e`, `\C-x`, `\M-x`,
/// `\n`, octal and `\x` numbers, ...).
fn parse(s: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if unit(s, &mut i, &mut out).is_none() {
            break;
        }
    }
    if out.is_empty() || i < s.len() {
        return Err(format!("invalid key sequence: {}", String::from_utf8_lossy(s)));
    }
    Ok(out)
}

/// Parses one character of a key sequence, at `s[*i]`, onto `out`.
fn unit(s: &[u8], i: &mut usize, out: &mut Vec<u8>) -> Option<()> {
    let c = *s.get(*i)?;
    *i += 1;
    let control = |c: u8| if c == b'?' { 0x7f } else { c.to_ascii_uppercase() ^ 0x40 };
    if c == b'^' && *i < s.len() {
        out.push(control(s[*i]));
        *i += 1;
        return Some(());
    }
    if c != b'\\' || *i == s.len() {
        out.push(c);
        return Some(());
    }
    let d = s[*i];
    *i += 1;
    let number = |i: &mut usize, radix: u32, max: usize| {
        let n = s[*i..]
            .iter()
            .take(max)
            .take_while(|&&c| (c as char).is_digit(radix))
            .count();
        let v = u32::from_str_radix(std::str::from_utf8(&s[*i..*i + n]).ok()?, radix).ok()?;
        *i += n;
        Some(v as u8)
    };
    match d {
        b'e' | b'E' => out.push(0x1b),
        b'n' => out.push(b'\n'),
        b't' => out.push(b'\t'),
        b'r' => out.push(b'\r'),
        b'a' => out.push(7),
        b'b' => out.push(8),
        b'f' => out.push(12),
        b'v' => out.push(11),
        b'C' | b'M' if s.get(*i) == Some(&b'-') => {
            *i += 1;
            let mut x = Vec::new();
            unit(s, i, &mut x)?;
            if d == b'C' {
                let last = x.pop()?;
                x.push(if last == 0x7f { 0x7f } else { control(last) & 0x1f });
            } else {
                out.push(0x1b);
            }
            out.extend(x);
        }
        b'x' => out.push(number(i, 16, 2)?),
        b'0'..=b'7' => {
            *i -= 1;
            out.push(number(i, 8, 3)?);
        }
        _ => out.push(d),
    }
    Some(())
}

/// Parses a key sequence given by the names of its keys, separated by
/// spaces: `Up`, `Ctrl-Right`, `Alt-.`, `Ctrl-X Ctrl-E`. Keys are named as in
/// `NAMES` or by their character, after modifiers (`Ctrl-`, `Alt-`,
/// `Shift-`, or `C-`, `M-`, `S-`; with `-` or `+`), all ignoring case. Gives
/// the bytes xterm sends for them, or None if `s` isn't written that way
/// (then it is a sequence as zsh writes them): some word must be a name or
/// have a modifier, and each other word must be a single character.
fn named(s: &[u8]) -> Option<Result<Vec<u8>, String>> {
    let words: Vec<&[u8]> = s.split(|&c| c == b' ').filter(|w| !w.is_empty()).collect();
    let parsed: Vec<_> = words.iter().map(|w| name_word(w)).collect();
    let one_char = |w: &[u8]| char_at(w).is_some_and(|(_, n)| n == w.len());
    if !parsed.iter().any(Option::is_some) || words.iter().zip(&parsed).any(|(w, p)| p.is_none() && !one_char(w)) {
        return None;
    }
    let mut out = Vec::new();
    for (w, p) in words.iter().zip(parsed) {
        match p.unwrap_or_else(|| Ok(w.to_vec())) {
            Ok(b) => out.extend(b),
            Err(()) => return Some(Err(format!("invalid key: {}", String::from_utf8_lossy(w)))),
        }
    }
    Some(Ok(out))
}

/// The keys that have names, and what xterm sends for each: a final byte of
/// `ESC [`, or the number before its `~` (or else the bytes).
const NAMES: &[(&str, NamedKey)] = &[
    ("backspace", NamedKey::Byte(0x7f)),
    ("bs", NamedKey::Byte(0x7f)),
    ("del", NamedKey::Tilde(3)),
    ("delete", NamedKey::Tilde(3)),
    ("down", NamedKey::Csi(b'B')),
    ("end", NamedKey::Csi(b'F')),
    ("enter", NamedKey::Byte(b'\r')),
    ("esc", NamedKey::Byte(0x1b)),
    ("escape", NamedKey::Byte(0x1b)),
    ("home", NamedKey::Csi(b'H')),
    ("ins", NamedKey::Tilde(2)),
    ("insert", NamedKey::Tilde(2)),
    ("left", NamedKey::Csi(b'D')),
    ("pagedown", NamedKey::Tilde(6)),
    ("pageup", NamedKey::Tilde(5)),
    ("pgdn", NamedKey::Tilde(6)),
    ("pgup", NamedKey::Tilde(5)),
    ("return", NamedKey::Byte(b'\r')),
    ("right", NamedKey::Csi(b'C')),
    ("space", NamedKey::Byte(b' ')),
    ("tab", NamedKey::Byte(b'\t')),
    ("up", NamedKey::Csi(b'A')),
];

#[derive(Clone, Copy)]
enum NamedKey {
    Csi(u8),
    Tilde(u8),
    Byte(u8),
}

/// The bytes of one named key (a word of `named`): None if it has no
/// modifier and isn't a name, Err if it can't be typed.
fn name_word(w: &[u8]) -> Option<Result<Vec<u8>, ()>> {
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    let mut rest = w;
    while rest.len() > 2 {
        let Some(sep) = rest[1..].iter().position(|&c| c == b'-' || c == b'+') else {
            break;
        };
        let flag = match rest[..sep + 1].to_ascii_lowercase().as_slice() {
            b"ctrl" | b"control" | b"c" => &mut ctrl,
            b"alt" | b"meta" | b"m" => &mut alt,
            b"shift" | b"s" => &mut shift,
            _ => break,
        };
        *flag = true;
        rest = &rest[sep + 2..];
    }
    let lower = rest.to_ascii_lowercase();
    let fkey = (lower.first() == Some(&b'f'))
        .then(|| std::str::from_utf8(&lower[1..]).ok()?.parse::<u8>().ok())
        .flatten()
        .filter(|n| (1..=12).contains(n));
    let key = match fkey {
        Some(n) => NamedKey::Tilde([11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24][n as usize - 1]),
        None => match NAMES.iter().find(|(n, _)| n.as_bytes() == lower) {
            Some(&(_, k)) => k,
            None if rest.len() == w.len() => return None,
            None => match char_at(rest) {
                Some((_, 1)) if rest.len() == 1 => NamedKey::Byte(rest[0]),
                // Another character, which can only be typed with Alt.
                Some((_, n)) if n == rest.len() && !ctrl && !shift => {
                    return Some(Ok([&b"\x1b"[..usize::from(alt)], rest].concat()));
                }
                _ => return Some(Err(())),
            },
        },
    };
    let code = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let mut out = Vec::new();
    match key {
        NamedKey::Csi(fin) if code == 1 => out.extend([0x1b, b'[', fin]),
        NamedKey::Csi(fin) => out.extend(format!("\x1b[1;{code}{}", fin as char).bytes()),
        NamedKey::Tilde(n) if code == 1 => out.extend(format!("\x1b[{n}~").bytes()),
        NamedKey::Tilde(n) => out.extend(format!("\x1b[{n};{code}~").bytes()),
        NamedKey::Byte(b) => {
            if alt {
                out.push(0x1b);
            }
            let b = match (b, ctrl, shift) {
                (b'\t', false, true) => return Some(Ok([&out[..], b"\x1b[Z"].concat())),
                (_, false, false) => b,
                (b'a'..=b'z' | b'A'..=b'Z', false, true) => b.to_ascii_uppercase(),
                (b' ', true, false) => 0,
                (b'?', true, false) => 0x7f,
                (b'@'..=b'_' | b'a'..=b'z', true, false) => b.to_ascii_uppercase() ^ 0x40,
                _ => return Some(Err(())),
            };
            out.push(b);
        }
    }
    Some(Ok(out))
}

/// Parses a key sequence as `bindkey` takes it: by the names of its keys
/// (`named`), or as zsh writes them (`parse`).
fn sequence(seq: &[u8]) -> Result<Keys, String> {
    let bytes = named(seq).unwrap_or_else(|| parse(seq))?;
    decode(&bytes).ok_or_else(|| format!("unknown key sequence: {}", String::from_utf8_lossy(seq)))
}

/// Decodes the bytes a terminal sends into keys, as rustyline does (for the
/// sequences of xterm and the Linux console).
fn decode(mut b: &[u8]) -> Option<Keys> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let (key, n) = first_key(b)?;
        out.push(key);
        b = &b[n..];
    }
    Some(out)
}

/// The first key in `b`, and the number of bytes it takes.
fn first_key(b: &[u8]) -> Option<(KeyEvent, usize)> {
    use {KeyEvent as E, Modifiers as M};
    if b[0] != 0x1b || b.len() == 1 {
        let (ch, n) = char_at(b)?;
        return Some((E::normalize(E::new(ch, M::NONE)), n));
    }
    match b[1] {
        b'[' => csi(&b[2..]).map(|(k, n)| (k, 2 + n)),
        b'O' if b.len() > 2 => Some((ss3(b[2])?, 3)),
        // Esc then another key: Alt and that key.
        0x1b => first_key(&b[1..]).map(|(E(k, m), n)| (E(k, m | M::ALT), 1 + n)),
        _ => {
            let (ch, n) = char_at(&b[1..])?;
            Some((E::normalize(E::new(ch, M::ALT)), 1 + n))
        }
    }
}

fn char_at(b: &[u8]) -> Option<(char, usize)> {
    let n = match b.first()? {
        0..0x80 => 1,
        0xc0..0xe0 => 2,
        0xe0..0xf0 => 3,
        _ => 4,
    };
    let c = std::str::from_utf8(b.get(..n)?).ok()?.chars().next()?;
    Some((c, n))
}

/// The key of `ESC O x`.
fn ss3(x: u8) -> Option<KeyEvent> {
    use {KeyCode as K, KeyEvent as E, Modifiers as M};
    Some(match x {
        b'A' => E(K::Up, M::NONE),
        b'B' => E(K::Down, M::NONE),
        b'C' => E(K::Right, M::NONE),
        b'D' => E(K::Left, M::NONE),
        b'H' => E(K::Home, M::NONE),
        b'F' => E(K::End, M::NONE),
        b'P'..=b'S' => E(K::F(x - b'P' + 1), M::NONE),
        _ => return None,
    })
}

/// The key of `ESC [ ...`, given what follows the `[`, and the number of
/// bytes it used.
fn csi(b: &[u8]) -> Option<(KeyEvent, usize)> {
    use {KeyCode as K, KeyEvent as E};
    let end = b.iter().position(|c| !c.is_ascii_digit() && *c != b';')?;
    let params: Vec<u8> = (std::str::from_utf8(&b[..end]).ok()?.split(';'))
        .map(|p| if p.is_empty() { Some(1) } else { p.parse().ok() })
        .collect::<Option<_>>()?;
    let mods = modifiers(params.get(1).copied().unwrap_or(1))?;
    let key = match b[end] {
        b'A' => K::Up,
        b'B' => K::Down,
        b'C' => K::Right,
        b'D' => K::Left,
        b'H' => K::Home,
        b'F' => K::End,
        b'Z' if end == 0 => K::BackTab,
        b'~' => match params.first()? {
            1 | 7 => K::Home,
            2 => K::Insert,
            3 => K::Delete,
            4 | 8 => K::End,
            5 => K::PageUp,
            6 => K::PageDown,
            n @ 11..=15 => K::F(n - 10),
            n @ 17..=21 => K::F(n - 11),
            n @ 23..=24 => K::F(n - 12),
            _ => return None,
        },
        _ => return None,
    };
    Some((E(key, mods), end + 1))
}

/// The modifiers of xterm's `;m` parameter.
fn modifiers(m: u8) -> Option<Modifiers> {
    use Modifiers as M;
    Some(match m {
        1 => M::NONE,
        2 => M::SHIFT,
        3 => M::ALT,
        4 => M::ALT_SHIFT,
        5 => M::CTRL,
        6 => M::CTRL_SHIFT,
        7 => M::CTRL_ALT,
        8 => M::CTRL_ALT_SHIFT,
        _ => return None,
    })
}

/// Writes keys as `bindkey` shows them: the bytes xterm sends, with `^X`
/// for control characters.
fn show(keys: &[KeyEvent]) -> String {
    use {KeyCode as K, Modifiers as M};
    let mut out = String::new();
    for &KeyEvent(k, m) in keys {
        let alt = m.contains(M::ALT);
        let m = m - M::ALT;
        let csi = |out: &mut String, fin: &str, num: u8| {
            let code = 1 + u8::from(m.contains(M::SHIFT)) + 2 * u8::from(alt) + 4 * u8::from(m.contains(M::CTRL));
            match (num, code) {
                (1, 1) => out.push_str(&format!("^[[{fin}")),
                (_, 1) => out.push_str(&format!("^[[{num}{fin}")),
                _ => out.push_str(&format!("^[[{num};{code}{fin}")),
            }
        };
        match k {
            K::Up | K::Down | K::Right | K::Left | K::Home | K::End => {
                let fin = match k {
                    K::Up => "A",
                    K::Down => "B",
                    K::Right => "C",
                    K::Left => "D",
                    K::Home => "H",
                    _ => "F",
                };
                csi(&mut out, fin, 1);
            }
            K::Insert | K::Delete | K::PageUp | K::PageDown => {
                let num = match k {
                    K::Insert => 2,
                    K::Delete => 3,
                    K::PageUp => 5,
                    _ => 6,
                };
                csi(&mut out, "~", num);
            }
            K::F(n @ 1..=4) if m.is_empty() && !alt => out.push_str(&format!("^[O{}", (b'O' + n) as char)),
            K::F(n) => csi(
                &mut out,
                "~",
                [0, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24][n.min(12) as usize],
            ),
            K::BackTab => out.push_str(if alt { "^[^[[Z" } else { "^[[Z" }),
            _ => {
                if alt {
                    out.push_str("^[");
                }
                match k {
                    K::Backspace => out.push_str("^?"),
                    K::Tab => out.push_str("^I"),
                    K::Enter => out.push_str("^M"),
                    K::Esc => out.push_str("^["),
                    K::Char(c) if m.contains(M::CTRL) => {
                        out.push('^');
                        out.push(c.to_ascii_uppercase());
                    }
                    K::Char(c @ ('^' | '\\' | '"')) => {
                        out.push('\\');
                        out.push(c);
                    }
                    K::Char(c) => out.push(c),
                    _ => out.push('?'),
                }
            }
        }
    }
    // Plain characters that would be read as the names of keys (`Up` is U
    // and p): the first is written in octal.
    if named(out.as_bytes()).is_some() {
        out = format!("\\{:03o}{}", out.as_bytes()[0], &out[1..]);
    }
    out
}

/// What the line editor's key handlers share with the completer and the
/// hinter.
#[derive(Default)]
pub struct State {
    /// The keymap in effect.
    keymap: HashMap<Keys, &'static str>,
    /// The version of the shell's `Keymap` it was made from.
    version: Option<u64>,
    /// The sequences given a handler in rustyline.
    bound: HashSet<Keys>,
    pub wordchars: Option<String>,
    /// Shared with the history store, for the prefix searches.
    pub search: Arc<Mutex<Search>>,
    /// What the completer is to do, for a key.
    pub pending: Option<Pending>,
    /// The edit the completer is making (see `Edit`).
    pub edit: Option<Edit>,
    /// What the last `insert-last-word` left, to replace it if repeated.
    last_word: Option<LastWord>,
    /// The position in the history (rustyline's history index) and the
    /// length of the history, as the hinter last saw them.
    pub history_index: usize,
    pub history_len: usize,
    /// The index of the history entry that the line was filled with by
    /// `accept-line-and-down-history`.
    pub prefilled: Option<usize>,
    /// Set by `accept-line-and-down-history`: the index of the history
    /// entry to fill the next line with.
    pub down: Option<usize>,
}

/// What a key asks the completer to do.
pub enum Pending {
    InsertLastWord,
    Edit(Edit),
}

struct LastWord {
    line: String,
    pos: usize,
    /// The history index of the entry the word came from.
    index: usize,
    len: usize,
}

impl State {
    /// Before each prompt: takes the keymap if it has changed, and forgets
    /// the last line's state.
    fn update(&mut self, keymap: &Keymap, wordchars: Option<String>) {
        if self.version != Some(keymap.version) {
            self.version = Some(keymap.version);
            self.keymap = defaults().collect();
            for (k, w) in &keymap.changes {
                match w {
                    Some(w) => self.keymap.insert(k.clone(), w),
                    None => self.keymap.remove(k),
                };
            }
        }
        self.wordchars = wordchars;
        self.pending = None;
        self.edit = None;
        self.last_word = None;
        self.down = None;
        self.prefilled = None;
        if let Ok(mut s) = self.search.lock() {
            *s = Search::default();
        }
    }

    fn is_word(&self, c: char) -> bool {
        c.is_alphanumeric() || self.wordchars.as_deref().unwrap_or(DEFAULT_WORDCHARS).contains(c)
    }

    fn run(&mut self, name: &str, n: RepeatCount, ctx: &EventContext) -> Option<Cmd> {
        let (_, action) = widget(name.as_bytes())?;
        let (line, pos) = (ctx.line(), ctx.pos());
        Some(match action {
            Action::Cmd(f) => f(n),
            Action::AcceptHint(_) if pos == line.len() && ctx.hint_text().is_some_and(|h| !h.is_empty()) => {
                Cmd::CompleteHint
            }
            Action::AcceptHint(f) => f(n),
            // zsh-autosuggestions accepts a word of the suggestion.
            Action::Word(WordOp::Forward) if pos == line.len() && ctx.hint_text().is_some_and(|h| !h.is_empty()) => {
                let full = [line, ctx.hint_text().unwrap_or_default()].concat();
                let t = word_target(&full, pos, WordOp::Forward, &|c| self.is_word(c));
                Cmd::Insert(1, full[pos..t].to_owned())
            }
            Action::Word(op) => match word_cmd(line, pos, op, n, |c| self.is_word(c)) {
                Ok(cmd) => cmd,
                Err(edit) => {
                    self.pending = Some(Pending::Edit(edit));
                    Cmd::Complete
                }
            },
            Action::HistorySearch(dir) => {
                if let Ok(mut s) = self.search.lock() {
                    if s.shown.as_deref() != Some(line) {
                        s.original = line.to_owned();
                    }
                    s.current = line.to_owned();
                }
                match dir {
                    SearchDirection::Reverse => Cmd::HistorySearchBackward,
                    SearchDirection::Forward => Cmd::HistorySearchForward,
                }
            }
            Action::InsertLastWord => {
                self.pending = Some(Pending::InsertLastWord);
                Cmd::Complete
            }
            Action::AcceptLineAndDownHistory => {
                self.down = if self.history_index < self.history_len {
                    Some(self.history_index)
                } else {
                    self.prefilled
                };
                Cmd::AcceptLine
            }
        })
    }

    /// Does `insert-last-word` for the completer: the start of the text to
    /// replace (up to `pos`) and the word to put there.
    pub fn insert_last_word(
        &mut self,
        line: &str,
        pos: usize,
        history: &dyn History,
        index: usize,
    ) -> Option<(usize, String)> {
        let (mut i, start) = match self.last_word.take() {
            Some(l) if l.line == line && l.pos == pos => (l.index, pos - l.len),
            _ => (index.min(history.len()), pos),
        };
        let word = loop {
            i = i.checked_sub(1)?;
            let entry = history.get(i, SearchDirection::Reverse).ok()??;
            if let Some(w) = last_word(&entry.entry) {
                break w.to_owned();
            }
        };
        self.last_word = Some(LastWord {
            line: [&line[..start], &word, &line[pos..]].concat(),
            pos: start + word.len(),
            index: i,
            len: word.len(),
        });
        Some((start, word))
    }
}

/// An edit that the completer makes (`Completer::update`), for what
/// rustyline's commands can't do: removes `range` and puts the cursor at
/// `pos`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edit {
    pub range: Range<usize>,
    pub pos: usize,
}

/// Whether a character is part of a word.
type WordChar = fn(char) -> bool;

/// Where a word motion or kill goes from `pos`, with `is_word` telling
/// the characters of words (zsh's widgets, and rustyline's with its own
/// definitions of words).
fn word_target(line: &str, pos: usize, op: WordOp, is_word: &dyn Fn(char) -> bool) -> usize {
    let back = |t: usize, word: bool| {
        (line[..t].char_indices().rev())
            .find(|&(_, c)| is_word(c) != word)
            .map_or(0, |(i, c)| i + c.len_utf8())
    };
    let fwd = |t: usize, word: bool| {
        (line[t..].char_indices())
            .find(|&(_, c)| is_word(c) != word)
            .map_or(line.len(), |(i, _)| t + i)
    };
    match op {
        WordOp::Backward | WordOp::BackwardKill => back(back(pos, false), true),
        WordOp::Forward => fwd(fwd(pos, true), false),
        WordOp::Kill => fwd(fwd(pos, false), true),
    }
}

/// A word motion or kill, as zsh's widgets do them, `n` times: a rustyline
/// command if one does it (so that kills go to the kill ring), or else an
/// edit for the completer to make.
///
/// rustyline gives the commands of key bindings the repeat count typed (1
/// by default), so only these will do: a search for the first occurrence
/// of a character, going to an end of the buffer, or one of rustyline's
/// own word motions when its definition of words gives the same place.
fn word_cmd(line: &str, pos: usize, op: WordOp, n: RepeatCount, is_word: impl Fn(char) -> bool) -> Result<Cmd, Edit> {
    use rustyline::{At, CharSearch as S};
    let mut t = pos;
    for _ in 0..n.max(1) {
        t = word_target(line, t, op, &is_word);
    }
    let kill = matches!(op, WordOp::Kill | WordOp::BackwardKill);
    let at = |i: usize| line[i..].chars().next();
    let before = |i: usize| line[..i].chars().next_back();
    // rustyline's words, which it measures in graphemes: the same as
    // characters in ASCII text. Its forward motion to the start of a word
    // gives the same place as zsh's only for its emacs words.
    let words: &[(Word, WordChar)] = match op {
        _ if n > 1 || !line.is_ascii() => &[],
        WordOp::Forward => &[(Word::Emacs, char::is_alphanumeric)],
        _ => &[
            (Word::Emacs, char::is_alphanumeric),
            (Word::Big, |c| !c.is_whitespace()),
        ],
    };
    let word = words
        .iter()
        .find(|w| word_target(line, pos, op, &w.1) == t)
        .map(|w| w.0);
    let search = if t == pos {
        return Ok(Cmd::Noop);
    } else if t < pos {
        // The character at `t`, or the one before it, found first going
        // back from the cursor.
        match (at(t), before(t)) {
            (Some(c), _) if !line[t + c.len_utf8()..pos].contains(c) => Some(S::Backward(c)),
            (_, Some(c)) if !line[t..pos].contains(c) => Some(S::BackwardAfter(c)),
            _ => None,
        }
    } else {
        // rustyline searches from after the character at the cursor.
        let from = pos + at(pos).map_or(0, char::len_utf8);
        match (at(t), before(t)) {
            (Some(c), _) if !line[from..t].contains(c) => Some(if kill { S::ForwardBefore(c) } else { S::Forward(c) }),
            (_, Some(c)) if kill && t - c.len_utf8() >= from && !line[from..t - c.len_utf8()].contains(c) => {
                Some(S::Forward(c))
            }
            _ => None,
        }
    };
    let mvt = match (search, word) {
        _ if t == 0 => Movement::BeginningOfBuffer,
        _ if t == line.len() => Movement::EndOfBuffer,
        (Some(cs), _) => Movement::ViCharSearch(1, cs),
        (None, Some(w)) if t < pos => Movement::BackwardWord(1, w),
        (None, Some(w)) => Movement::ForwardWord(1, if kill { At::AfterEnd } else { At::Start }, w),
        (None, None) if kill => {
            return Err(Edit {
                range: t.min(pos)..t.max(pos),
                pos: t.min(pos),
            });
        }
        (None, None) => {
            return Err(Edit {
                range: pos..pos,
                pos: t,
            });
        }
    };
    Ok(if kill { Cmd::Kill(mvt) } else { Cmd::Move(mvt) })
}

/// The last word of a command line, for `insert-last-word`: words are
/// separated by blanks and by the operators `;&|<>()`, which aren't words,
/// and quotes are kept.
fn last_word(line: &str) -> Option<&str> {
    let b = line.as_bytes();
    let mut last = None;
    let mut start = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => {
                if let Some(s) = start.take() {
                    last = Some(s..i);
                }
            }
            b'#' if start.is_none() => break,
            _ => {
                start.get_or_insert(i);
                match c {
                    b'\\' => i += 1,
                    b'\'' => i += b[i + 1..].iter().position(|&c| c == b'\'').map_or(b.len(), |n| n + 1),
                    b'"' => {
                        let mut j = i + 1;
                        while j < b.len() && b[j] != b'"' {
                            j += if b[j] == b'\\' { 2 } else { 1 };
                        }
                        i = j;
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }
    if let Some(s) = start {
        last = Some(s..b.len());
    }
    line.get(last?)
}

/// The key handler of every sequence in the keymap, and of the keys the
/// completion menu uses.
struct Dispatch {
    menu: Arc<Mutex<Menu>>,
    state: Arc<Mutex<State>>,
}

impl ConditionalEventHandler for Dispatch {
    fn handle(&self, evt: &Event, n: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let Event::KeySeq(keys) = evt else { return None };
        if let [key] = keys.as_slice()
            && let Some(cmd) = super::remote::escape_key(*key, ctx.line(), ctx.pos())
        {
            return Some(cmd);
        }
        if ctx.mode() == EditMode::Vi
            && let Some(key) = keys.last()
        {
            super::integration::vi_key(*key, ctx.input_mode());
        }
        if let [key] = keys.as_slice()
            && let Some(cmd) = menu::key(&self.menu, *key, ctx)
        {
            return Some(cmd);
        }
        if ctx.mode() != EditMode::Emacs {
            return None;
        }
        let mut st = self.state.lock().ok()?;
        let name = *st.keymap.get(keys)?;
        st.run(name, n, ctx)
    }
}

/// The keys that nothing else binds, which do what rustyline does: in vi
/// mode, the cursor's shape follows the input mode they lead to. (Both
/// handlers first look for the SSH mode's escapes.)
struct ViCursor;

impl ConditionalEventHandler for ViCursor {
    fn handle(&self, evt: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        if let Event::KeySeq(keys) = evt
            && let [key] = keys.as_slice()
            && let Some(cmd) = super::remote::escape_key(*key, ctx.line(), ctx.pos())
        {
            return Some(cmd);
        }
        if ctx.mode() == EditMode::Vi
            && let Event::KeySeq(keys) = evt
            && let Some(key) = keys.last()
        {
            super::integration::vi_key(*key, ctx.input_mode());
        }
        None
    }
}

/// Binds the keys of the menu and the default keymap.
pub fn bind<H: Helper, I: History>(ed: &mut Editor<H, I>, menu: &Arc<Mutex<Menu>>, state: &Arc<Mutex<State>>) {
    ed.bind_sequence(Event::Any, EventHandler::Conditional(Box::new(ViCursor)));
    let Ok(mut st) = state.lock() else { return };
    let keys = (menu::keys().into_iter().map(|k| vec![k])).chain(defaults().map(|(k, _)| k));
    for k in keys {
        bind_one(ed, menu, state, &mut st, k);
    }
}

fn bind_one<H: Helper, I: History>(
    ed: &mut Editor<H, I>,
    menu: &Arc<Mutex<Menu>>,
    state: &Arc<Mutex<State>>,
    st: &mut State,
    keys: Keys,
) {
    if st.bound.insert(keys.clone()) {
        let handler = Dispatch {
            menu: Arc::clone(menu),
            state: Arc::clone(state),
        };
        ed.bind_sequence(Event::KeySeq(keys), EventHandler::Conditional(Box::new(handler)));
    }
}

/// Before each prompt: gives the line editor the shell's keymap, and binds
/// the sequences that are new.
pub fn update<H: Helper, I: History>(
    ed: &mut Editor<H, I>,
    menu: &Arc<Mutex<Menu>>,
    state: &Arc<Mutex<State>>,
    keymap: &Keymap,
    wordchars: Option<String>,
) {
    let Ok(mut st) = state.lock() else { return };
    st.update(keymap, wordchars);
    for (k, _) in &keymap.changes {
        bind_one(ed, menu, state, &mut st, k.clone());
    }
}

/// `bindkey`: shows and changes the key bindings, as zsh's.
pub fn bindkey(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

pub fn run(sh: &mut Shell, name: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let mut remove = false;
    let mut commands = false;
    let mut select = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        let mut chars = a[1..].iter();
        while let Some(&c) = chars.next() {
            match c {
                b'e' | b'v' => {
                    sh.options.set(if c == b'e' { Opt::Emacs } else { Opt::Vi }, true);
                    select = true;
                }
                b'r' => remove = true,
                b'L' => commands = true,
                b'M' => {
                    let rest = chars.as_slice();
                    let map = if rest.is_empty() {
                        i += 1;
                        args.get(i - 1).map(Vec::as_slice)
                    } else {
                        Some(rest)
                    };
                    chars = [].iter();
                    match map {
                        Some(b"emacs" | b"main") => {}
                        Some(m) => {
                            sh.berr(name, format!("no such keymap `{}'", String::from_utf8_lossy(m)));
                            return Ok(1);
                        }
                        None => {
                            sh.berr(name, "-M: argument expected");
                            return Ok(1);
                        }
                    }
                }
                _ => {
                    sh.berr(name, format!("bad option: -{}", c as char));
                    return Ok(1);
                }
            }
        }
    }
    let args = &args[i..];
    let keys = |sh: &Shell, seq: &[u8]| sequence(seq).map_err(|e| sh.berr(name, e)).ok();
    if remove {
        let mut status = 0;
        for seq in args {
            match keys(sh, seq) {
                Some(k) => sh.keymap.set(k, None),
                None => status = 1,
            }
        }
        return Ok(status);
    }
    let line = |seq: &str, w: &str| {
        let seq = format!("\"{seq}\"");
        if commands {
            format!("bindkey {seq} {w}\n")
        } else {
            format!("{seq} {w}\n")
        }
    };
    match args {
        [] if select => Ok(0),
        [] => {
            let text: String = sh.keymap.bindings().iter().map(|(s, w)| line(s, w)).collect();
            Ok(sh.out_status(text.as_bytes()))
        }
        [seq] => {
            let Some(k) = keys(sh, seq) else { return Ok(1) };
            let shown = show(&k);
            let w = (sh.keymap.bindings().into_iter())
                .find(|(s, _)| *s == shown)
                .map_or("undefined-key", |(_, w)| w);
            Ok(sh.out_status(line(&shown, w).as_bytes()))
        }
        [seq, w] => match bind_widget(sh, seq, w) {
            Ok(()) => Ok(0),
            Err(e) => {
                sh.berr(name, e);
                Ok(1)
            }
        },
        _ => {
            sh.berr(name, "too many arguments");
            Ok(1)
        }
    }
}

/// Binds the key sequence `seq` (as `bindkey` takes it) to the widget `w`
/// (for `bindkey` and config.toml's `bindkey` table).
pub fn bind_widget(sh: &mut Shell, seq: &[u8], w: &[u8]) -> Result<(), String> {
    let k = sequence(seq)?;
    let (w, _) = widget(w).ok_or_else(|| format!("no such widget `{}'", String::from_utf8_lossy(w)))?;
    sh.keymap.set(k, Some(w));
    Ok(())
}

/// The widget names, for completion.
pub fn widget_names() -> impl Iterator<Item = &'static str> {
    WIDGETS.iter().map(|w| w.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode as K;
    use KeyEvent as E;
    use Modifiers as M;

    fn keys(s: &str) -> Keys {
        decode(&parse(s.as_bytes()).unwrap()).unwrap()
    }

    #[test]
    fn widgets_sorted_and_defaults_valid() {
        assert!(WIDGETS.windows(2).all(|w| w[0].0 < w[1].0));
        for (seq, w) in DEFAULTS {
            assert!(widget(w.as_bytes()).is_some(), "{w}");
            assert_eq!(show(&keys(seq)), *seq);
        }
    }

    #[test]
    fn sequences() {
        assert_eq!(keys("^W"), [E(K::Char('W'), M::CTRL)]);
        assert_eq!(keys("^?"), [E(K::Backspace, M::NONE)]);
        assert_eq!(keys("^[."), [E(K::Char('.'), M::ALT)]);
        assert_eq!(keys("\\e."), keys("^[."));
        assert_eq!(keys("\\M-."), keys("^[."));
        assert_eq!(keys("\\C-w"), keys("^W"));
        assert_eq!(keys("^[[A"), [E(K::Up, M::NONE)]);
        assert_eq!(keys("^[OA"), [E(K::Up, M::NONE)]);
        assert_eq!(keys("^[[1;5C"), [E(K::Right, M::CTRL)]);
        assert_eq!(keys("^[[3~"), [E(K::Delete, M::NONE)]);
        assert_eq!(keys("^[^?"), [E(K::Backspace, M::ALT)]);
        assert_eq!(keys("^X^E"), [E(K::Char('X'), M::CTRL), E(K::Char('E'), M::CTRL)]);
        assert_eq!(keys("ab"), [E(K::Char('a'), M::NONE), E(K::Char('b'), M::NONE)]);
        assert_eq!(keys("\\033[Z"), [E(K::BackTab, M::NONE)]);
        assert_eq!(keys("^[^[[A"), [E(K::Up, M::ALT)]);
        assert_eq!(show(&keys("^[[1;5C")), "^[[1;5C");
        assert_eq!(show(&keys("^[OA")), "^[[A");
        assert_eq!(show(&keys("^I")), "^I");
        assert_eq!(show(&keys("^[OP")), "^[OP");
        assert_eq!(show(&keys("^[[15~")), "^[[15~");
        assert!(decode(&parse(b"^[[9x").unwrap()).is_none());
        assert!(parse(b"").is_err());
    }

    #[test]
    fn names() {
        let seq = |s: &str| sequence(s.as_bytes());
        assert_eq!(seq("Up"), Ok(keys("^[[A")));
        assert_eq!(seq("UP"), seq("up"));
        assert_eq!(seq("Ctrl-Right"), Ok(keys("^[[1;5C")));
        assert_eq!(seq("C-S-left"), Ok(keys("^[[1;6D")));
        assert_eq!(seq("Alt+Up"), Ok(keys("^[[1;3A")));
        assert_eq!(seq("Delete"), Ok(keys("^[[3~")));
        assert_eq!(seq("Ctrl-PageUp"), Ok(keys("^[[5;5~")));
        assert_eq!(seq("F1"), Ok(keys("^[OP")));
        assert_eq!(seq("Shift-F5"), Ok(keys("^[[15;2~")));
        assert_eq!(seq("Ctrl-w"), Ok(keys("^W")));
        assert_eq!(seq("Ctrl-?"), Ok(keys("^?")));
        assert_eq!(seq("Ctrl-Space"), Ok(keys("^@")));
        assert_eq!(seq("Alt-."), Ok(keys("^[.")));
        assert_eq!(seq("M-b"), Ok(keys("^[b")));
        assert_eq!(seq("Alt-B"), Ok(keys("^[B")));
        assert_eq!(seq("Alt-Shift-b"), Ok(keys("^[B")));
        assert_eq!(seq("Alt-Backspace"), Ok(keys("^[^?")));
        assert_eq!(seq("Ctrl-_"), Ok(keys("^_")));
        assert_eq!(seq("Alt--"), Ok(keys("^[-")));
        assert_eq!(seq("Shift-Tab"), Ok(keys("^[[Z")));
        assert_eq!(seq("Enter"), Ok(keys("^M")));
        assert_eq!(seq("Ctrl-X Ctrl-E"), Ok(keys("^X^E")));
        assert_eq!(seq("Ctrl-X e"), Ok(keys("^Xe")));
        assert_eq!(seq("Alt-é"), Ok(keys("\\eé")));
        assert!(seq("Ctrl-Tab").is_err());
        assert!(seq("Ctrl-1").is_err());
        assert!(seq("Ctrl-Bogus").is_err());
        // Not written with names: as zsh writes them.
        assert_eq!(seq("ab"), Ok(keys("ab")));
        assert_eq!(seq("a b"), Ok(keys("a b")));
        assert_eq!(seq("C-"), Ok(keys("C-")));
        assert_eq!(seq("^X Up"), Ok(keys("^X Up")));
        assert_eq!(seq("\\Up"), Ok(keys("Up")));
        // Shown so that they are read back as the same keys.
        for s in ["Up", "up", "C-x", "a Up", "F1", "Tab"] {
            let k = keys(s);
            assert_eq!(seq(&show(&k)), Ok(k), "{s}");
        }
        assert_eq!(show(&keys("Up")), "\\125p");
    }

    #[test]
    fn keymap_changes() {
        let mut k = Keymap::default();
        k.set(keys("^[[A"), Some("up-line-or-history"));
        k.set(keys("^X^E"), Some("undo"));
        k.set(keys("^W"), None);
        let b = k.bindings();
        assert!(b.contains(&("^[[A".into(), "up-line-or-history")));
        assert!(b.contains(&("^X^E".into(), "undo")));
        assert!(!b.iter().any(|(s, _)| s == "^W"));
        assert_eq!(k.state().len(), 3);
        // Back to the default: no longer a change.
        k.set(keys("^[OA"), Some("history-beginning-search-backward"));
        assert_eq!(k.state().len(), 2);
        assert_eq!(
            restore_command(b"^W"),
            b"__luish_internal bindkey '^W' backward-kill-word\n"
        );
        assert_eq!(restore_command(b"^X^E"), b"__luish_internal bindkey -r '^X^E'\n");
    }

    fn apply(line: &str, pos: usize, op: WordOp, wordchars: &str) -> (String, usize) {
        use rustyline::CharSearch as S;
        let is_word = |c: char| c.is_alphanumeric() || wordchars.contains(c);
        let (m, kill) = match word_cmd(line, pos, op, 1, is_word) {
            Ok(Cmd::Noop) => return (line.into(), pos),
            Ok(Cmd::Move(m)) => (m, false),
            Ok(Cmd::Kill(m)) => (m, true),
            Ok(_) => unreachable!(),
            Err(e) => return ([&line[..e.range.start], &line[e.range.end..]].concat(), e.pos),
        };
        // As rustyline does it.
        let from = pos + line[pos..].chars().next().map_or(0, char::len_utf8);
        let t = match m {
            Movement::BeginningOfBuffer => 0,
            Movement::EndOfBuffer => line.len(),
            Movement::ViCharSearch(1, S::Backward(c)) => line[..pos].rfind(c).unwrap(),
            Movement::ViCharSearch(1, S::BackwardAfter(c)) => line[..pos].rfind(c).unwrap() + c.len_utf8(),
            Movement::ViCharSearch(1, S::Forward(c)) if kill => from + line[from..].find(c).unwrap() + c.len_utf8(),
            Movement::ViCharSearch(1, S::Forward(c) | S::ForwardBefore(c)) => from + line[from..].find(c).unwrap(),
            Movement::BackwardWord(1, w) | Movement::ForwardWord(1, _, w) => {
                let def: &dyn Fn(char) -> bool = match w {
                    Word::Emacs => &|c: char| c.is_alphanumeric(),
                    _ => &|c: char| !c.is_whitespace(),
                };
                word_target(line, pos, op, def)
            }
            _ => unreachable!(),
        };
        if kill {
            let (a, b) = (t.min(pos), t.max(pos));
            ([&line[..a], &line[b..]].concat(), a)
        } else {
            (line.into(), t)
        }
    }

    #[test]
    fn words() {
        use WordOp::*;
        let no_slash = "*?_-.[]~=&;!#%^(){}<>";
        // WORDCHARS without `/`: Ctrl-W removes one path component.
        assert_eq!(
            apply("ls /usr/lib/x86", 15, BackwardKill, no_slash),
            ("ls /usr/lib/".into(), 12)
        );
        assert_eq!(
            apply("ls /usr/lib/", 12, BackwardKill, no_slash),
            ("ls /usr/".into(), 8)
        );
        // zsh's default has `/`.
        assert_eq!(
            apply("ls /usr/lib/", 12, BackwardKill, DEFAULT_WORDCHARS),
            ("ls ".into(), 3)
        );
        assert_eq!(apply("ls", 2, BackwardKill, ""), ("".into(), 0));
        assert_eq!(apply("", 0, BackwardKill, ""), ("".into(), 0));
        // Repeated characters: the search counts them.
        assert_eq!(apply("a/a/a/aa", 8, BackwardKill, ""), ("a/a/a/".into(), 6));
        assert_eq!(apply("echo aa bb", 10, Backward, ""), ("echo aa bb".into(), 8));
        // Forward to the start of the next word, as zsh's forward-word.
        assert_eq!(apply("echo aa bb", 0, Forward, ""), ("echo aa bb".into(), 5));
        assert_eq!(apply("echo aa bb", 8, Forward, ""), ("echo aa bb".into(), 10));
        // kill-word: to the end of the word.
        assert_eq!(apply("echo aa bb", 4, Kill, ""), ("echo bb".into(), 4));
        assert_eq!(apply("echo aa aa", 4, Kill, ""), ("echo aa".into(), 4));
        assert_eq!(apply("x é/é", 7, BackwardKill, ""), ("x é/".into(), 5));
        // Neither the first character nor the one before it can be searched
        // for: an edit.
        assert_eq!(apply("ls aaa  ", 8, BackwardKill, ""), ("ls ".into(), 3));
        // rustyline's own words, when they give the same place.
        let cmd = |line: &str, pos, op, wordchars: &str| {
            word_cmd(line, pos, op, 1, |c: char| c.is_alphanumeric() || wordchars.contains(c))
        };
        assert_eq!(
            cmd("ls aaa  ", 8, BackwardKill, ""),
            Ok(Cmd::Kill(Movement::BackwardWord(1, Word::Emacs)))
        );
        assert_eq!(apply("cd /tmp/test/", 13, BackwardKill, ""), ("cd /tmp/".into(), 8));
        assert_eq!(
            cmd("cd /tmp/test/", 13, BackwardKill, ""),
            Ok(Cmd::Kill(Movement::BackwardWord(1, Word::Emacs)))
        );
        assert_eq!(
            cmd("ls a-a/a  ", 10, BackwardKill, "-/"),
            Ok(Cmd::Kill(Movement::BackwardWord(1, Word::Big)))
        );
        // Neither: an edit.
        assert!(cmd("ls a-a/a- ", 10, BackwardKill, "/").is_err());
        assert_eq!(apply("ls a-a/a- ", 10, BackwardKill, "/"), ("ls a-".into(), 5));
        assert_eq!(apply("ls  aa bb", 2, Kill, ""), ("ls bb".into(), 2));
        assert_eq!(apply("ls  aa bb", 2, Forward, ""), ("ls  aa bb".into(), 4));
        assert_eq!(apply("ab ab ab", 0, Forward, ""), ("ab ab ab".into(), 3));
        assert_eq!(apply("ab ab ab", 8, Backward, ""), ("ab ab ab".into(), 6));
        assert_eq!(apply("aa aa", 0, Kill, ""), (" aa".into(), 0));
    }

    #[test]
    fn last_words() {
        assert_eq!(last_word("ls -l foo"), Some("foo"));
        assert_eq!(last_word("echo 'a b' \"c d\""), Some("\"c d\""));
        assert_eq!(last_word("make &"), Some("make"));
        assert_eq!(last_word("cat x | wc -l;"), Some("-l"));
        assert_eq!(last_word("echo a\\ b"), Some("a\\ b"));
        assert_eq!(last_word("ls # comment"), Some("ls"));
        assert_eq!(last_word("  "), None);
        assert_eq!(last_word("echo x>out"), Some("out"));
    }
}
