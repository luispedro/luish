//! Syntax highlighting for the line editor.
//!
//! Like the completer, the highlighter never touches `Shell`: it uses the
//! `Names` snapshot, plus the text of the command read so far (for `PS2`
//! lines) and the colours, all set before each prompt. It gives each byte
//! a role (a name in `crate::style::ROLES`) with a rough tokenizer that
//! follows quoting, expansions, operators, redirections, here-documents,
//! comments and reserved words, and gives command names a role by what they
//! are (built-in, function, alias, ...). A name under the cursor is not
//! marked as unknown, since it may still be being typed. Likewise, `$NAME`
//! and `${NAME}` are marked when `NAME` is not set, unless an earlier part
//! of the text assigns it (`NAME=`, also as an argument, or `for NAME`).
//!
//! The colours are the styles of the roles, with those of the modifiers
//! (such as `error`) added. Highlighting is off with `setopt
//! editor.no_highlight`, or if `$NO_COLOR` is set and not empty.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

use rustyline::highlight::{CmdKind, Highlighter};

use super::complete::{PRECOMMANDS, RESERVED, ShellHelper, is_executable};
use crate::style::{ROLES, Role, Style};
use crate::sys;

/// The roles the highlighter gives.
pub mod role {
    use crate::style::Role;

    pub const BUILTIN: Role = Role::of("command.builtin");
    pub const FUNCTION: Role = Role::of("command.function");
    pub const ALIAS: Role = Role::of("command.alias");
    pub const EXTERNAL: Role = Role::of("command.external");
    pub const PRECOMMAND: Role = Role::of("command.precommand");
    pub const DIRECTORY: Role = Role::of("command.directory");
    pub const HISTORY: Role = Role::of("command.history");
    pub const UNKNOWN: Role = Role::of("command.unknown");
    pub const KEYWORD: Role = Role::of("keyword");
    pub const ARG: Role = Role::of("arg");
    pub const OPTION: Role = Role::of("arg.option");
    pub const SINGLE: Role = Role::of("string.single");
    pub const DOUBLE: Role = Role::of("string.double");
    pub const HEREDOC: Role = Role::of("string.heredoc");
    pub const ESCAPE: Role = Role::of("string.escape");
    pub const VAR: Role = Role::of("var");
    pub const SPECIAL: Role = Role::of("var.special");
    pub const UNSET: Role = Role::of("var.unset");
    pub const SUBST: Role = Role::of("subst.command");
    pub const PROCESS: Role = Role::of("subst.process");
    pub const ARITH: Role = Role::of("subst.arith");
    pub const TILDE: Role = Role::of("expand.tilde");
    pub const BRACE: Role = Role::of("expand.brace");
    pub const GLOB: Role = Role::of("expand.glob");
    pub const OP: Role = Role::of("op");
    pub const CONTROL: Role = Role::of("op.control");
    pub const PIPE: Role = Role::of("op.pipe");
    pub const REDIR: Role = Role::of("redir");
    pub const FD: Role = Role::of("redir.fd");
    pub const ASSIGN: Role = Role::of("assign");
    pub const COMMENT: Role = Role::of("comment");
    pub const SELECTED: Role = Role::of("menu.selected");
    pub const DESCRIPTION: Role = Role::of("menu.description");
    pub const SUGGESTION: Role = Role::of("suggestion");
}

/// The modifiers, as bits of [`Cell::mods`] by their position here. Their
/// styles are added to the role's.
pub const MODIFIERS: [Role; 4] = [
    Role::of("error"),
    Role::of("path"),
    Role::of("path.prefix"),
    Role::of("match"),
];

/// What a byte of the line is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Cell {
    pub role: Role,
    /// Bits for [`MODIFIERS`].
    pub mods: u8,
}

/// What a command name is, as far as the line editor can tell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandKind {
    /// A built-in, also an extension's.
    Builtin,
    Function,
    /// An alias, or a suffix alias.
    Alias,
    External,
    /// A directory that `setopt cd.auto` changes to.
    Directory,
    /// A history reference, expanded once the line is entered.
    History,
    Unknown,
}

impl CommandKind {
    fn role(self) -> Role {
        match self {
            CommandKind::Builtin => role::BUILTIN,
            CommandKind::Function => role::FUNCTION,
            CommandKind::Alias => role::ALIAS,
            CommandKind::External => role::EXTERNAL,
            CommandKind::Directory => role::DIRECTORY,
            CommandKind::History => role::HISTORY,
            CommandKind::Unknown => role::UNKNOWN,
        }
    }
}

/// The styles of the roles, resolved, with their SGR parameters (without
/// `ESC [` and `m`).
#[derive(Debug)]
pub struct Colors {
    /// By position in [`ROLES`].
    styles: Vec<Style>,
    sgr: Vec<Vec<u8>>,
    /// The SGR parameters of the cells with modifiers that have been drawn.
    merged: RefCell<HashMap<Cell, Vec<u8>>>,
}

/// The built-in colours for a dark background.
impl Default for Colors {
    fn default() -> Colors {
        let styles = crate::style::Styles::default();
        let r = styles.resolver(Some("default-dark"));
        Colors::new(|n| r.get(n))
    }
}

impl Colors {
    /// The colours of the styles that `style` gives.
    pub fn new(style: impl Fn(&str) -> Style) -> Colors {
        let styles: Vec<Style> = ROLES.iter().map(|r| style(r)).collect();
        Colors {
            sgr: styles.iter().map(|s| s.sgr().into_bytes()).collect(),
            styles,
            merged: RefCell::default(),
        }
    }

    /// The colours of the completion menu and suggestions with `$NO_COLOR`
    /// set: the selection in reverse video, and suggestions in grey.
    pub fn no_color() -> Colors {
        Colors::new(|n| match n {
            "menu.selected" => Style::parse(&["reverse"]).unwrap_or_default(),
            "suggestion" => Style::parse(&["bright-black"]).unwrap_or_default(),
            _ => Style::default(),
        })
    }

    /// The SGR parameters of `role`.
    pub fn sgr(&self, role: Role) -> &[u8] {
        role.index().map_or(&[], |i| &self.sgr[i])
    }

    /// The SGR parameters of `cell`: its role's, with its modifiers'
    /// added.
    fn cell(&self, cell: Cell) -> Cow<'_, [u8]> {
        if cell.mods == 0 {
            return Cow::Borrowed(self.sgr(cell.role));
        }
        let mut merged = self.merged.borrow_mut();
        let sgr = merged.entry(cell).or_insert_with(|| {
            let mut s = cell.role.index().map(|i| self.styles[i].clone()).unwrap_or_default();
            for (bit, m) in MODIFIERS.iter().enumerate() {
                if cell.mods & (1 << bit) != 0
                    && let Some(i) = m.index()
                {
                    s = s.add(&self.styles[i]);
                }
            }
            s.sgr().into_bytes()
        });
        Cow::Owned(sgr.clone())
    }
}

/// What the highlighter needs besides `Names`, refreshed before each prompt.
#[derive(Default)]
pub struct State {
    pub colors: std::rc::Rc<Colors>,
    /// Whether the line is highlighted (the colours are also those of the
    /// completion menu and suggestions).
    pub on: bool,
    /// The earlier lines of an incomplete command.
    pub context: Vec<u8>,
    /// What each command name looked up since the prompt is.
    pub known: RefCell<HashMap<Vec<u8>, CommandKind>>,
}

/// What `classify` needs to know about the shell.
pub struct Facts<'a> {
    pub command: &'a dyn Fn(&[u8]) -> CommandKind,
    /// Whether a variable is set.
    pub is_set: &'a dyn Fn(&[u8]) -> bool,
    /// `setopt expand.braces`.
    pub braces: bool,
    /// Not `set -f`.
    pub glob: bool,
}

/// What follows `for` or `case`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    None,
    For,
    ForName,
    Case,
    CaseWord,
    /// The names after `function`.
    Function,
    /// Inside `[[ ... ]]`, where `<`, `>`, `(`, `)`, `&&` and `||` are
    /// operators of the expression.
    Cond,
    /// The options after `__luish_cache`, up to its `{`.
    Cache,
}

struct Scan<'a> {
    s: &'a [u8],
    cls: Vec<Cell>,
    facts: &'a Facts<'a>,
    /// The names assigned so far in the text.
    assigned: Vec<Vec<u8>>,
    /// The cursor's position in the text.
    cursor: Option<usize>,
    /// Pending here-documents: the delimiter, and whether tabs are stripped.
    heredocs: Vec<(Vec<u8>, bool)>,
    in_backquote: bool,
}

/// A scanned word: its unquoted text, and whether it had quotes or
/// expansions.
struct Word {
    end: usize,
    text: Vec<u8>,
    quoted: bool,
    expanded: bool,
}

/// If a word is an assignment (`NAME=`, `NAME+=` or `NAME[...]=`), the end
/// of the name and the position of the `=`.
fn assignment(raw: &[u8]) -> Option<(usize, usize)> {
    let eq = raw.iter().position(|&c| c == b'=')?;
    let n = raw
        .iter()
        .position(|&c| !crate::lexer::is_name_char(c))
        .unwrap_or(raw.len());
    let name = raw[..eq].strip_suffix(b"+").unwrap_or(&raw[..eq]);
    let ok = crate::lexer::is_valid_name(&raw[..n]) && (n == name.len() || raw[n] == b'[' && name.ends_with(b"]"));
    ok.then_some((n, eq))
}

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

fn ends_word(c: u8) -> bool {
    b" \t\n;&|()<>".contains(&c)
}

/// Whether `<` or `>` at `i` starts a process substitution.
fn is_procsubst(s: &[u8], i: usize) -> bool {
    matches!(s[i], b'<' | b'>') && s.get(i + 1) == Some(&b'(')
}

/// Whether `name` (in `${...}` or after `$`) is a special parameter or a
/// positional one.
fn is_special(name: &[u8]) -> bool {
    match name {
        [c] => b"@*#?-$!".contains(c) || c.is_ascii_digit(),
        _ => !name.is_empty() && name.iter().all(u8::is_ascii_digit),
    }
}

impl Scan<'_> {
    fn paint(&mut self, a: usize, b: usize, role: Role) {
        let b = b.min(self.s.len());
        for c in &mut self.cls[a..b] {
            c.role = role;
        }
    }

    /// Paints the bytes in `a..b` that have no role yet.
    fn paint_plain(&mut self, a: usize, b: usize, role: Role) {
        for c in &mut self.cls[a..b.min(self.s.len())] {
            if c.role == Role::NONE {
                c.role = role;
            }
        }
    }

    /// Whether the byte at `i` has no role yet: in a word, that it is
    /// literal and not quoted.
    fn bare(&self, i: usize) -> bool {
        self.cls[i].role == Role::NONE
    }

    fn find(&self, from: usize, c: u8) -> Option<usize> {
        self.s[from..].iter().position(|&b| b == c).map(|p| from + p)
    }

    /// Scans commands from `i` up to `end` (a `)` or backquote closing a
    /// nested list) or the end of the text. Returns where it stopped.
    fn list(&mut self, mut i: usize, end: Option<u8>) -> usize {
        let s = self.s;
        let mut cmd = true;
        let mut precommand = false;
        let mut after = After::None;
        let mut pattern = false;
        while i < s.len() {
            let c = s[i];
            if Some(c) == end && !(pattern && c == b')') {
                return i;
            }
            if after == After::Cond && matches!(c, b'<' | b'>' | b'(' | b')' | b'&' | b'|' | b'0'..=b'9') {
                let len = match s.get(i + 1) {
                    Some(&n) if n == c && matches!(c, b'&' | b'|') => 2,
                    _ if c.is_ascii_digit() => 0,
                    _ => 1,
                };
                if len > 0 {
                    self.paint(i, i + len, role::OP);
                    i += len;
                } else {
                    i = self.command_word(i, &mut cmd, &mut precommand, &mut after, &mut pattern);
                }
                continue;
            }
            match c {
                b' ' | b'\t' => i += 1,
                b'\n' => {
                    i = self.heredoc_bodies(i + 1);
                    if !pattern && after == After::None {
                        cmd = true;
                    }
                    precommand = false;
                }
                b'#' => {
                    let e = self.find(i, b'\n').unwrap_or(s.len());
                    self.paint(i, e, role::COMMENT);
                    i = e;
                }
                b';' | b'&' | b'|' => {
                    let len = if s.get(i + 1) == Some(&c) { 2 } else { 1 };
                    let r = match (c, len) {
                        _ if pattern => role::OP,
                        (b';', 2) => role::OP,
                        (b'|', 1) => role::PIPE,
                        _ => role::CONTROL,
                    };
                    self.paint(i, i + len, r);
                    i += len;
                    if pattern && c == b'|' && len == 1 {
                        continue;
                    }
                    pattern = c == b';' && len == 2;
                    cmd = !pattern;
                    precommand = false;
                    after = After::None;
                }
                b'(' => {
                    self.paint(i, i + 1, role::OP);
                    i += 1;
                    let close = i + s[i..].iter().take_while(|&&c| is_blank(c)).count();
                    if !pattern && s.get(close) == Some(&b')') {
                        // A function definition: the body follows.
                        self.paint(close, close + 1, role::OP);
                        i = close + 1;
                        cmd = true;
                    } else if cmd && !pattern {
                        i = self.list(i, Some(b')'));
                        if s.get(i) == Some(&b')') {
                            self.paint(i, i + 1, role::OP);
                            i += 1;
                        }
                        cmd = false;
                    }
                }
                b')' => {
                    self.paint(i, i + 1, role::OP);
                    i += 1;
                    if pattern {
                        pattern = false;
                        cmd = true;
                    }
                }
                b'<' | b'>' if !is_procsubst(s, i) => i = self.redirect(i, i),
                b'0'..=b'9' => {
                    let d = i + s[i..].iter().take_while(|c| c.is_ascii_digit()).count();
                    if matches!(s.get(d), Some(b'<' | b'>')) && !is_procsubst(s, d) {
                        i = self.redirect(i, d);
                    } else {
                        i = self.command_word(i, &mut cmd, &mut precommand, &mut after, &mut pattern);
                    }
                }
                _ => {
                    let e = self.command_word(i, &mut cmd, &mut precommand, &mut after, &mut pattern);
                    if e == i {
                        // A backquote closing an outer substitution.
                        return i;
                    }
                    i = e;
                }
            }
        }
        i
    }

    /// Scans a word in a list and classifies it by its position.
    fn command_word(
        &mut self,
        start: usize,
        cmd: &mut bool,
        precommand: &mut bool,
        after: &mut After,
        pattern: &mut bool,
    ) -> usize {
        let w = self.word(start);
        let e = w.end;
        let plain = !w.quoted && !w.expanded;
        let text = &w.text[..];
        let raw = &self.s[start..e];
        let assign = assignment(raw);
        if let Some((name, _)) = assign {
            self.assigned.push(raw[..name].to_vec());
        }
        // An array, `a=(x y)`, also as an argument (`local a=(x y)`).
        let array = assign.is_some_and(|(_, eq)| eq + 1 == raw.len()) && self.s.get(e) == Some(&b'(');
        if *pattern {
            if plain && text == b"esac" {
                self.paint(start, e, role::KEYWORD);
                *pattern = false;
                *cmd = false;
            } else {
                self.globs(start, e);
            }
            return e;
        }
        match *after {
            After::For => {
                self.assigned.push(w.text.clone());
                *after = After::ForName;
                return e;
            }
            After::Case => {
                *after = After::CaseWord;
                return e;
            }
            After::ForName | After::CaseWord if plain && text == b"in" => {
                self.paint(start, e, role::KEYWORD);
                *pattern = *after == After::CaseWord;
                *after = After::None;
                return e;
            }
            After::Function if !(plain && text == b"{") => {
                self.paint_plain(start, e, role::FUNCTION);
                return e;
            }
            After::Function => *after = After::None,
            After::Cond => {
                if plain && text == b"]]" {
                    self.paint(start, e, role::KEYWORD);
                    *after = After::None;
                }
                return e;
            }
            After::Cache if plain && text == b"{" => {
                self.paint(start, e, role::KEYWORD);
                *after = After::None;
                *cmd = true;
                return e;
            }
            After::Cache => {
                if array {
                    self.paint(start, e, role::ASSIGN);
                    return self.array(e);
                }
                return e;
            }
            After::ForName if plain && text == b"do" => {
                self.paint(start, e, role::KEYWORD);
                *after = After::None;
                *cmd = true;
                return e;
            }
            _ => {}
        }
        if !*cmd || (*precommand && raw.starts_with(b"-")) {
            if array {
                return self.array(e);
            }
            self.arg(start, e, true);
            return e;
        }
        if plain && text == b"__luish_cache" {
            self.paint(start, e, role::KEYWORD);
            *after = After::Cache;
            *cmd = false;
            *precommand = false;
        } else if plain && (RESERVED.contains(&text) || text == b"{" || text == b"}" || text == b"!") {
            self.paint(start, e, role::KEYWORD);
            match text {
                b"for" => *after = After::For,
                b"case" => *after = After::Case,
                b"function" => *after = After::Function,
                b"[[" => *after = After::Cond,
                _ => {}
            }
            *cmd = !matches!(text, b"for" | b"case" | b"fi" | b"done" | b"esac" | b"}" | b"[[");
            *precommand = false;
        } else if let Some((_, eq)) = assign {
            self.paint(start, start + eq + 1, role::ASSIGN);
            if array {
                return self.array(e);
            }
            self.tildes(start + eq + 1, e, true);
        } else if w.expanded {
            *cmd = false;
            *precommand = false;
        } else {
            let rest = &self.s[e..];
            let definition = rest.iter().find(|&&c| !is_blank(c)) == Some(&b'(');
            let kind = if definition {
                CommandKind::Function
            } else {
                (self.facts.command)(text)
            };
            let known = kind != CommandKind::Unknown;
            *precommand = known && !definition && PRECOMMANDS.contains(&text);
            let r = if *precommand { role::PRECOMMAND } else { kind.role() };
            if known || !self.cursor.is_some_and(|c| (start..=e).contains(&c)) {
                self.paint_plain(start, e, r);
            }
            *cmd = *precommand;
        }
        e
    }

    /// Paints the argument `start..end`: its expansions (tilde, braces
    /// and globs), then the rest as an argument, or as an option if it
    /// starts with `-` and `option`.
    fn arg(&mut self, start: usize, end: usize, option: bool) {
        self.tildes(start, end, false);
        if self.facts.braces {
            self.braces(start, end);
        }
        if self.facts.glob {
            self.globs(start, end);
        }
        let s = &self.s[start..end];
        let r = match option && s.len() > 1 && s[0] == b'-' {
            true => role::OPTION,
            false => role::ARG,
        };
        self.paint_plain(start, end, r);
    }

    /// Paints a tilde prefix at `start`, and if `colons` (in an
    /// assignment's value) also after each `:` up to `end`.
    fn tildes(&mut self, start: usize, end: usize, colons: bool) {
        let mut i = start;
        loop {
            if i < end && self.s[i] == b'~' {
                let e = i + self.s[i..end].iter().take_while(|&&c| c != b'/' && c != b':').count();
                if (i..e).all(|k| self.bare(k)) {
                    self.paint(i, e, role::TILDE);
                }
            }
            match self.s[i..end].iter().position(|&c| c == b':') {
                Some(p) if colons => i += p + 1,
                _ => return,
            }
        }
    }

    /// Paints the unquoted pattern characters in `start..end`: `*`, `?`
    /// and bracket expressions.
    fn globs(&mut self, start: usize, end: usize) {
        let s = self.s;
        // So that each `[` doesn't look to the end for a `]`.
        let last = (start..end).rev().find(|&k| s[k] == b']' && self.bare(k));
        let mut i = start;
        while i < end {
            if !self.bare(i) {
                i += 1;
                continue;
            }
            match s[i] {
                b'*' | b'?' => self.paint(i, i + 1, role::GLOB),
                b'[' => {
                    let mut j = i + 1;
                    if matches!(s.get(j), Some(b'!' | b'^')) {
                        j += 1;
                    }
                    if s.get(j) == Some(&b']') {
                        j += 1;
                    }
                    let close = last
                        .filter(|&l| l >= j)
                        .and_then(|l| (j..=l).find(|&k| s[k] == b']' && self.bare(k)));
                    if let Some(close) = close {
                        self.paint_plain(i, close + 1, role::GLOB);
                        i = close;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Paints the braces and separators of brace expansions in
    /// `start..end` (`{a,b}`, `{1..3}`).
    fn braces(&mut self, start: usize, end: usize) {
        let s = self.s;
        // The open braces: where each is, and its separators.
        let mut open: Vec<(usize, Vec<std::ops::Range<usize>>)> = Vec::new();
        let mut k = start;
        while k < end {
            if self.bare(k) {
                match s[k] {
                    b'{' => open.push((k, Vec::new())),
                    b'}' => {
                        if let Some((o, seps)) = open.pop().filter(|o| !o.1.is_empty()) {
                            self.paint(o, o + 1, role::BRACE);
                            self.paint(k, k + 1, role::BRACE);
                            for r in seps {
                                self.paint(r.start, r.end, role::BRACE);
                            }
                        }
                    }
                    b',' => open.last_mut().into_iter().for_each(|o| o.1.push(k..k + 1)),
                    b'.' if s.get(k + 1) == Some(&b'.') && k + 1 < end && self.bare(k + 1) => {
                        open.last_mut().into_iter().for_each(|o| o.1.push(k..k + 2));
                        k += 1;
                    }
                    _ => {}
                }
            }
            k += 1;
        }
    }

    /// Scans the elements of an array, from its `(` to the `)`.
    fn array(&mut self, open: usize) -> usize {
        let s = self.s;
        self.paint(open, open + 1, role::OP);
        let mut i = open + 1;
        while i < s.len() {
            match s[i] {
                b' ' | b'\t' | b'\n' => i += 1,
                b'#' => {
                    let e = self.find(i, b'\n').unwrap_or(s.len());
                    self.paint(i, e, role::COMMENT);
                    i = e;
                }
                b')' => {
                    self.paint(i, i + 1, role::OP);
                    return i + 1;
                }
                _ => {
                    let e = self.word(i).end;
                    if e == i {
                        return i;
                    }
                    self.arg(i, e, false);
                    i = e;
                }
            }
        }
        i
    }

    /// Scans a word, painting its quoted parts and expansions.
    fn word(&mut self, start: usize) -> Word {
        let s = self.s;
        let mut w = Word {
            end: start,
            text: Vec::new(),
            quoted: false,
            expanded: false,
        };
        let mut j = start;
        while j < s.len() {
            let c = s[j];
            if is_procsubst(s, j) {
                w.expanded = true;
                j = self.nested(j, 2, role::PROCESS);
                continue;
            }
            if ends_word(c) || (c == b'`' && self.in_backquote) {
                break;
            }
            match c {
                b'\\' => {
                    if s.get(j + 1) != Some(&b'\n') {
                        w.quoted = true;
                        w.text.extend(s.get(j + 1));
                        self.paint(j, j + 2, role::ESCAPE);
                    }
                    j += 2;
                }
                b'\'' => {
                    w.quoted = true;
                    let e = self.find(j + 1, b'\'').map_or(s.len(), |e| e + 1);
                    w.text.extend_from_slice(&s[j + 1..e.max(j + 1)]);
                    self.paint(j, e, role::SINGLE);
                    j = e;
                }
                b'"' => {
                    w.quoted = true;
                    j = self.double_quote(j, &mut w);
                }
                b'$' => {
                    let e = self.dollar(j);
                    if e == j + 1 {
                        w.text.push(b'$');
                    } else {
                        w.expanded = true;
                    }
                    j = e;
                }
                b'`' => {
                    w.expanded = true;
                    j = self.backquote(j);
                }
                _ => {
                    w.text.push(c);
                    j += 1;
                }
            }
        }
        w.end = j.min(s.len());
        w
    }

    fn double_quote(&mut self, start: usize, w: &mut Word) -> usize {
        let s = self.s;
        self.paint(start, start + 1, role::DOUBLE);
        let mut k = start + 1;
        while k < s.len() {
            match s[k] {
                b'"' => {
                    self.paint(k, k + 1, role::DOUBLE);
                    return k + 1;
                }
                b'\\' => {
                    match s.get(k + 1) {
                        Some(&n) if b"$`\"\\\n".contains(&n) => {
                            self.paint(k, k + 2, role::ESCAPE);
                            w.text.push(n);
                        }
                        _ => {
                            self.paint(k, k + 2, role::DOUBLE);
                            w.text.push(b'\\');
                        }
                    }
                    k += 2;
                }
                b'$' => {
                    let e = self.dollar(k);
                    if e == k + 1 {
                        self.paint(k, e, role::DOUBLE);
                        w.text.push(b'$');
                    } else {
                        w.expanded = true;
                    }
                    k = e;
                }
                b'`' if self.in_backquote => return k,
                b'`' => {
                    w.expanded = true;
                    k = self.backquote(k);
                }
                c => {
                    self.paint(k, k + 1, role::DOUBLE);
                    w.text.push(c);
                    k += 1;
                }
            }
        }
        s.len()
    }

    /// Scans an expansion starting with `$`. Returns `i + 1` if there is
    /// none (a literal `$`).
    fn dollar(&mut self, i: usize) -> usize {
        let s = self.s;
        let (e, r) = match (s.get(i + 1), s.get(i + 2)) {
            (Some(b'('), Some(b'(')) => {
                let mut depth = 0;
                let mut k = i + 1;
                while k < s.len() {
                    match s[k] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    k += 1;
                    if depth == 0 {
                        break;
                    }
                }
                (k, role::ARITH)
            }
            (Some(b'('), _) => return self.nested(i, 2, role::SUBST),
            (Some(b'{'), _) => {
                let mut depth = 0;
                let mut k = i + 1;
                while k < s.len() {
                    match s[k] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        b'\\' => k += 1,
                        b'\'' => k = self.find(k + 1, b'\'').unwrap_or(s.len()),
                        _ => {}
                    }
                    k += 1;
                    if depth == 0 {
                        break;
                    }
                }
                let k = k.min(s.len());
                if depth == 0 && crate::lexer::is_valid_name(&s[i + 2..k - 1]) {
                    return self.name(i, i + 2..k - 1, k);
                }
                let special = depth == 0 && is_special(&s[i + 2..k - 1]);
                (k, if special { role::SPECIAL } else { role::VAR })
            }
            (Some(&c), _) if c.is_ascii_alphabetic() || c == b'_' => {
                let e = i
                    + 1
                    + s[i + 1..]
                        .iter()
                        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                        .count();
                return self.name(i, i + 1..e, e);
            }
            (Some(&c), _) if c.is_ascii_digit() || b"@*#?-$!".contains(&c) => (i + 2, role::SPECIAL),
            _ => return i + 1,
        };
        self.paint(i, e, r);
        e
    }

    /// Paints the expansion `i..e` of the variable named by `name`, marking
    /// it if the variable is unset (but not while the cursor is on it).
    fn name(&mut self, i: usize, name: std::ops::Range<usize>, e: usize) -> usize {
        let name = &self.s[name];
        let set = (self.facts.is_set)(name)
            || self.assigned.iter().any(|a| a == name)
            || self.cursor.is_some_and(|c| (i..=e).contains(&c));
        self.paint(i, e, if set { role::VAR } else { role::UNSET });
        e
    }

    /// Scans a nested list whose opening delimiter, `len` bytes long, is
    /// at `i` (`$(`, `<(` or `>(`), up to its `)`.
    fn nested(&mut self, i: usize, len: usize, r: Role) -> usize {
        self.paint(i, i + len, r);
        let saved = std::mem::replace(&mut self.in_backquote, false);
        let k = self.list(i + len, Some(b')'));
        self.in_backquote = saved;
        if self.s.get(k) == Some(&b')') {
            self.paint(k, k + 1, r);
            return k + 1;
        }
        k
    }

    fn backquote(&mut self, i: usize) -> usize {
        self.paint(i, i + 1, role::SUBST);
        let saved = std::mem::replace(&mut self.in_backquote, true);
        let k = self.list(i + 1, Some(b'`'));
        self.in_backquote = saved;
        if self.s.get(k) == Some(&b'`') {
            self.paint(k, k + 1, role::SUBST);
            return k + 1;
        }
        k
    }

    /// Scans a redirection whose fd number (if any) starts at `start` and
    /// whose operator starts at `op`, and its target.
    fn redirect(&mut self, start: usize, op: usize) -> usize {
        let s = self.s;
        let mut k = op + 1;
        let mut heredoc = None;
        let mut dup = false;
        match (s[op], s.get(k)) {
            (b'<', Some(b'<')) => {
                k += 1;
                heredoc = Some(s.get(k) == Some(&b'-'));
                if heredoc == Some(true) {
                    k += 1;
                }
            }
            (_, Some(b'&')) => {
                dup = true;
                k += 1;
            }
            (b'<', Some(b'>')) | (b'>', Some(b'>' | b'|')) => k += 1,
            _ => {}
        }
        self.paint(start, op, role::FD);
        self.paint(op, k, role::REDIR);
        while k < s.len() && is_blank(s[k]) {
            k += 1;
        }
        if k >= s.len() || ends_word(s[k]) {
            return k;
        }
        let w = self.word(k);
        if dup {
            self.paint_plain(k, w.end, role::FD);
        }
        if let Some(strip) = heredoc {
            self.heredocs.push((w.text, strip));
        }
        w.end
    }

    /// Paints the bodies of pending here-documents, starting at `i` (after a
    /// newline). Returns where the commands continue.
    fn heredoc_bodies(&mut self, mut i: usize) -> usize {
        for (delim, strip) in std::mem::take(&mut self.heredocs) {
            while i < self.s.len() {
                let e = self.find(i, b'\n').unwrap_or(self.s.len());
                let mut line = &self.s[i..e];
                if strip {
                    line = &line[line.iter().take_while(|&&c| c == b'\t').count()..];
                }
                if line == &delim[..] {
                    self.paint(i, e, role::REDIR);
                    i = e + 1;
                    break;
                }
                self.paint(i, e + 1, role::HEREDOC);
                i = e + 1;
            }
        }
        i.min(self.s.len())
    }
}

/// Gives each byte of `text` its role.
pub fn classify(text: &[u8], cursor: Option<usize>, facts: &Facts) -> Vec<Cell> {
    let mut sc = Scan {
        s: text,
        cls: vec![Cell::default(); text.len()],
        facts,
        assigned: Vec::new(),
        cursor,
        heredocs: Vec::new(),
        in_backquote: false,
    };
    let mut i = 0;
    while i < text.len() {
        // Stray closing parentheses and backquotes.
        i = sc.list(i, None);
        if i < text.len() {
            sc.paint(i, i + 1, role::OP);
            i += 1;
        }
    }
    sc.cls
}

/// Adds SGR sequences to `line` for the cells `cls`, changing colour only
/// at character boundaries.
pub fn render(line: &[u8], cls: &[Cell], colors: &Colors) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 32);
    let mut cur: Cow<[u8]> = Cow::Borrowed(b"");
    let mut last = None;
    for (&b, &c) in line.iter().zip(cls) {
        if b & 0xc0 != 0x80 && last != Some(c) {
            last = Some(c);
            let want = colors.cell(c);
            if want != cur {
                if !cur.is_empty() {
                    out.extend_from_slice(b"\x1b[0m");
                }
                if !want.is_empty() {
                    out.extend_from_slice(b"\x1b[");
                    out.extend_from_slice(&want);
                    out.push(b'm');
                }
                cur = want;
            }
        }
        out.push(b);
    }
    if !cur.is_empty() {
        out.extend_from_slice(b"\x1b[0m");
    }
    out
}

impl ShellHelper {
    /// What the command `name` is, in the order the shell looks it up.
    pub(super) fn command_kind(&self, name: &[u8]) -> CommandKind {
        if self.names.history_expand && (name.contains(&b'!') || name.starts_with(b"^")) {
            return CommandKind::History;
        }
        if let Some(&k) = self.highlight.known.borrow().get(name) {
            return k;
        }
        let k = if name.contains(&b'/') {
            let found = match (name.strip_prefix(b"~/"), &self.names.home) {
                (Some(rest), Some(home)) => is_executable(&[&home[..], b"/", rest].concat()),
                _ => is_executable(name),
            };
            found.then_some(CommandKind::External)
        } else if self.names.aliases.contains(name) || self.names.aliases.for_suffix(name).is_some() {
            Some(CommandKind::Alias)
        } else if crate::builtins::lookup(name).is_some_and(|(_, special)| special) {
            Some(CommandKind::Builtin)
        } else if self.names.functions.iter().any(|c| c == name) {
            Some(CommandKind::Function)
        } else if crate::builtins::names().any(|b| b == name) || self.names.builtins.iter().any(|c| c == name) {
            Some(CommandKind::Builtin)
        } else {
            let found = crate::path::search(&self.names.path, name).is_some_and(|(_, _, exec)| exec);
            found.then_some(CommandKind::External)
        };
        let k = k
            .or_else(|| (self.names.autocd && self.is_autocd_dir(name)).then_some(CommandKind::Directory))
            .unwrap_or(CommandKind::Unknown);
        self.highlight.known.borrow_mut().insert(name.to_vec(), k);
        k
    }
}

impl ShellHelper {
    /// Whether `name` is a directory that `setopt cd.auto` changes to (see
    /// `Shell::autocd_target`).
    fn is_autocd_dir(&self, name: &[u8]) -> bool {
        let name = match (name.strip_prefix(b"~/"), &self.names.home) {
            (Some(rest), Some(home)) => [&home[..], b"/", rest].concat(),
            _ => name.to_vec(),
        };
        if sys::is_dir(&name) {
            return true;
        }
        let dotted = name == b"." || name == b".." || name.starts_with(b"./") || name.starts_with(b"../");
        !dotted
            && !name.starts_with(b"/")
            && (self.names.cdpath.split(|&c| c == b':'))
                .any(|p| !p.is_empty() && sys::is_dir(&[p, b"/", &name].concat()))
    }
}

impl Highlighter for ShellHelper {
    /// The line in colour, followed by the right prompt if it fits.
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let mut out = self.colored(line, pos);
        if let Some(right) = self.right.draw(&self.prompt, line) {
            out.to_mut().push_str(&right);
        }
        out
    }

    /// Every change repaints, including cursor moves, since the word under
    /// the cursor is coloured differently, and the right prompt may have
    /// to go or come back. Notes whether the line is being accepted (the
    /// last refresh, without the hint), and drops the autosuggestion when
    /// it isn't drawn.
    fn highlight_char(&self, _line: &str, _pos: usize, kind: CmdKind) -> bool {
        self.right.accepting.set(kind == CmdKind::ForcedRefresh);
        if kind != CmdKind::Other {
            self.right.hint.set(0);
        }
        self.highlight.on || self.right.is_set()
    }
}

impl ShellHelper {
    fn colored<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        if !self.highlight.on {
            return Cow::Borrowed(line);
        }
        let colors = &self.highlight.colors;
        let context = &self.highlight.context;
        let text = [&context[..], line.as_bytes()].concat();
        let facts = Facts {
            command: &|name| self.command_kind(name),
            is_set: &|name| self.names.vars.iter().any(|v| v == name),
            braces: self.names.braces,
            glob: self.names.glob,
        };
        let cls = classify(&text, Some(context.len() + pos), &facts);
        String::from_utf8(render(line.as_bytes(), &cls[context.len()..], colors))
            .map_or(Cow::Borrowed(line), Cow::Owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(n: &[u8]) -> CommandKind {
        match n {
            b"echo" | b"cd" => CommandKind::Builtin,
            b"cat" | b"ls" | b"sudo" => CommandKind::External,
            b"f" => CommandKind::Function,
            b"ll" => CommandKind::Alias,
            _ => CommandKind::Unknown,
        }
    }

    fn cells(text: &str, cursor: Option<usize>) -> Vec<Cell> {
        let facts = Facts {
            command: &known,
            is_set: &|n| !n.starts_with(b"UN"),
            braces: true,
            glob: true,
        };
        classify(text.as_bytes(), cursor, &facts)
    }

    /// One letter per byte, by the top-level role: `k`eyword, `c`ommand
    /// (`u` if unknown), `s`tring, `v`ar (`x` if unset), `$` substitution,
    /// `g` expansion, `o`perator, `r`edirection, `#` comment, `a`ssignment,
    /// `.` an argument or nothing.
    fn classes(text: &str) -> String {
        classes_at(text, None)
    }

    fn classes_at(text: &str, cursor: Option<usize>) -> String {
        (cells(text, cursor).into_iter())
            .map(|c| match c.role {
                role::UNKNOWN => 'u',
                role::UNSET => 'x',
                r => match r.index().map(|i| ROLES[i].split('.').next().unwrap_or_default()) {
                    None | Some("arg") => '.',
                    Some("keyword") => 'k',
                    Some("command") => 'c',
                    Some("string") => 's',
                    Some("var") => 'v',
                    Some("subst") => '$',
                    Some("expand") => 'g',
                    Some("op") => 'o',
                    Some("redir") => 'r',
                    Some("comment") => '#',
                    Some("assign") => 'a',
                    Some(r) => unreachable!("{r}"),
                },
            })
            .collect()
    }

    /// The runs of bytes with a role, as `text:role`.
    fn roles(text: &str) -> String {
        let cls = cells(text, None);
        let mut out = Vec::new();
        let mut i = 0;
        while i < cls.len() {
            let e = i + cls[i..].iter().take_while(|c| **c == cls[i]).count();
            if let Some(r) = cls[i].role.index() {
                out.push(format!("{}:{}", &text[i..e], ROLES[r]));
            }
            i = e;
        }
        out.join(" ")
    }

    #[test]
    fn commands_and_keywords() {
        assert_eq!(classes("if echo if; then nope; fi"), "kk.cccc...o.kkkk.uuuuo.kk");
        assert_eq!(classes("x=1 echo"), "aa1.cccc".replace('1', "."));
        assert_eq!(classes("sudo -E ls"), "cccc....cc");
        assert_eq!(classes("echo a | cat && ls"), "cccc...o.ccc.oo.cc");
        assert_eq!(classes("f() { ls; }"), "coo.k.cco.k");
        assert_eq!(classes("function a-b c { ls; }"), "kkkkkkkk.ccc.c.k.cco.k");
        assert_eq!(classes("function f()\n{ nope; }"), "kkkkkkkk.coo.k.uuuuo.k");
        assert_eq!(
            classes("[[ -f a && ( b<c || 1 -lt 2 ) ]] >x && ls"),
            "kk......oo.o..o..oo.........o.kk.r..oo.cc"
        );
        assert_eq!(classes("[[ $x = \"y\" ]]"), "kk.vv...sss.kk");
        assert_eq!(classes("echo [[ ]]"), "cccc......");
        assert_eq!(classes("! ls"), "k.cc");
        assert_eq!(classes("(ls)"), "occo");
        assert_eq!(classes("e\"ch\"o"), "cssssc");
        assert_eq!(
            classes("__luish_cache env=(A) { ls; }"),
            "kkkkkkkkkkkkk.aaaao.o.k.cco.k"
        );
        assert_eq!(
            classes("__luish_cache files=($x)\n{ nope; }"),
            "kkkkkkkkkkkkk.aaaaaaovvo.k.uuuuo.k"
        );
    }

    #[test]
    fn for_and_case() {
        assert_eq!(classes("for i in a; do ls; done"), "kkk...kk..o.kk.cco.kkkk");
        assert_eq!(classes("for i do ls; done"), "kkk...kk.cco.kkkk");
        assert_eq!(
            classes("case $x in a|b) ls;; (c) echo;; esac"),
            "kkkk.vv.kk..o.o.ccoo.o.o.ccccoo.kkkk"
        );
    }

    #[test]
    fn quotes_and_expansions() {
        assert_eq!(classes("echo 'a $b' \"c $d\""), "cccc.ssssss.sssvvs");
        assert_eq!(classes("echo ${a:-x} $1 $$ $((1+2))"), "cccc.vvvvvvv.vv.vv.$$$$$$$$");
        assert_eq!(classes("echo $(ls -l) `nope`"), "cccc.$$cc...$.$uuuu$");
        assert_eq!(classes("echo \"$(ls \")\")\""), "cccc.s$$cc.sss$s");
        assert_eq!(classes("echo a\\ b $"), "cccc..ss...");
        assert_eq!(classes("echo x # c"), "cccc...###");
        assert_eq!(classes("echo x#y"), "cccc....");
    }

    #[test]
    fn unset_variables() {
        assert_eq!(
            classes("echo $UNX ${UNX} $x ${UNX:-a} $1"),
            "cccc.xxxx.xxxxxx.vv.vvvvvvvvv.vv"
        );
        assert_eq!(classes("echo \"$UNX\" ${#UNX}"), "cccc.sxxxxs.vvvvvvv");
        // Unless assigned earlier in the text.
        assert_eq!(classes("echo $UNX; UNX=1"), "cccc.xxxxo.aaaa.");
        assert_eq!(classes("UNX=1; echo $UNX"), "aaaa.o.cccc.vvvv");
        assert_eq!(classes("echo UNX=1; echo $UNX"), "cccc......o.cccc.vvvv");
        assert_eq!(
            classes("for UNX in a; do echo $UNX; done"),
            "kkk.....kk..o.kk.cccc.vvvvo.kkkk"
        );
        // Arrays: the elements are arguments.
        assert_eq!(
            classes("a=(x \"y\" $x) b+=(1) c[1]=2 d[2]+=3; echo; sudo e=(ls #c\n)"),
            "aao..sss.vvo.aaao.o.aaaaa..aaaaaa.o.cccco.cccc.aao...##.o"
        );
        // Nor while the cursor is on it.
        assert_eq!(classes_at("echo $UN", Some(8)), "cccc.vvv");
        assert_eq!(classes_at("echo $UN ", Some(9)), "cccc.xxx.");
    }

    #[test]
    fn redirections() {
        assert_eq!(classes("ls 2>&1 >out <in"), "cc.rrrr.r....r..");
        assert_eq!(classes(">x ls"), "r..cc");
        assert_eq!(classes("cat <<EOF\nif\nEOF\nls"), "ccc.rr....sssrrr.cc");
        assert_eq!(classes("cat <<-E; ls\n\tE\n"), "ccc.rrr.o.cc.rr.");
    }

    #[test]
    fn nesting_and_continuation() {
        assert_eq!(
            classes("echo $(case a in a) ls;; esac)"),
            "cccc.$$kkkk...kk..o.ccoo.kkkk$"
        );
        assert_eq!(classes("x=$(ls) cat"), "aa$$cc$.ccc");
        assert_eq!(classes("echo 'a\nb' ls"), "cccc.sssss...");
        // The last line of a command continued from earlier lines.
        assert_eq!(classes("echo 'a\nb' nope"), "cccc.sssss.....");
        assert_eq!(classes("if ls\nthen nope"), "kk.cc.kkkk.uuuu");
        assert_eq!(classes("ec\\\nho"), "cccccc");
    }

    #[test]
    fn word_being_typed() {
        // Not unknown while the cursor is on it (it may be unfinished).
        assert_eq!(classes_at("ech", Some(3)), "...");
        assert_eq!(classes_at("ech", Some(0)), "...");
        assert_eq!(classes_at("ech ", Some(4)), "uuu.");
        assert_eq!(classes_at("nope; ec", Some(2)), "....o.uu");
        assert_eq!(classes_at("nope; ec", Some(8)), "uuuuo...");
        // Known commands are coloured as they are.
        assert_eq!(classes_at("ls", Some(2)), "cc");
    }

    #[test]
    fn command_roles() {
        assert_eq!(
            roles("sudo -E ls --all -"),
            "sudo:command.precommand -E:arg.option ls:command.external --all:arg.option -:arg"
        );
        assert_eq!(
            roles("f; ll; cd x"),
            "f:command.function ;:op.control ll:command.alias ;:op.control cd:command.builtin x:arg"
        );
        assert_eq!(
            roles("g() { f; }; function h { f; }"),
            "g:command.function ():op {:keyword f:command.function ;:op.control }:keyword ;:op.control \
             function:keyword h:command.function {:keyword f:command.function ;:op.control }:keyword"
        );
        assert_eq!(
            roles("a | b || c && d; e & f"),
            "a:command.unknown |:op.pipe b:command.unknown ||:op.control c:command.unknown &&:op.control \
             d:command.unknown ;:op.control e:command.unknown &:op.control f:command.function"
        );
        assert_eq!(
            roles("case x in a*|[b]) ;; esac"),
            "case:keyword in:keyword *:expand.glob |:op [b]:expand.glob ):op ;;:op esac:keyword"
        );
        assert_eq!(
            roles("f 2>&1 >&- 3<x"),
            "f:command.function 2:redir.fd >&:redir 1:redir.fd >&:redir -:redir.fd 3:redir.fd <:redir"
        );
    }

    #[test]
    fn string_and_var_roles() {
        assert_eq!(
            roles("echo 'a' \"b\\$c\\d\" \\e $'x'"),
            "echo:command.builtin 'a':string.single \"b:string.double \\$:string.escape c\\d\":string.double \
             \\e:string.escape $:arg 'x':string.single"
        );
        assert_eq!(
            roles("cat <<E\n$x\nE\n"),
            "cat:command.external <<:redir $x\n:string.heredoc E:redir"
        );
        assert_eq!(
            roles("echo $1 $? ${#} ${10} ${x:-1} $((1+$x))"),
            "echo:command.builtin $1:var.special $?:var.special ${#}:var.special ${10}:var.special ${x:-1}:var \
             $((1+$x)):subst.arith"
        );
        assert_eq!(
            roles("echo $(ls) `ls` <(ls) --in=>(cat)"),
            "echo:command.builtin $(:subst.command ls:command.external ):subst.command `:subst.command \
             ls:command.external `:subst.command <(:subst.process ls:command.external ):subst.process \
             --in=:arg.option >(:subst.process cat:command.external ):subst.process"
        );
        // `2<(` is a word, not a redirection of fd 2.
        assert_eq!(classes("cat < <(ls) 2<(ls)"), "ccc.r.$$cc$..$$cc$");
    }

    #[test]
    fn expansion_roles() {
        assert_eq!(
            roles("ls ~ ~u/x a~ '~' x=~/a:~/b; y=~/a:~/b"),
            "ls:command.external ~:expand.tilde ~u:expand.tilde /x:arg a~:arg '~':string.single x=~/a:~/b:arg \
             ;:op.control y=:assign ~:expand.tilde ~:expand.tilde"
        );
        assert_eq!(
            roles("echo {a,b} {1..3} x{a,{b,c}}y {} {a} '{a,b}' \\{a,b}"),
            "echo:command.builtin {:expand.brace a:arg ,:expand.brace b:arg }:expand.brace {:expand.brace 1:arg \
             ..:expand.brace 3:arg }:expand.brace x:arg {:expand.brace a:arg ,{:expand.brace b:arg ,:expand.brace \
             c:arg }}:expand.brace y:arg {}:arg {a}:arg '{a,b}':string.single \\{:string.escape a,b}:arg"
        );
        assert_eq!(
            roles("ls *.c a?b [!a]x [] \"*\" \\* $x*"),
            "ls:command.external *:expand.glob .c:arg a:arg ?:expand.glob b:arg [!a]:expand.glob x:arg []:arg \
             \"*\":string.double \\*:string.escape $x:var *:expand.glob"
        );
        assert_eq!(roles("a=(*.c -x)"), "a=:assign (:op *:expand.glob .c:arg -x:arg ):op");
        // Without `expand.braces`, and with `set -f`.
        let facts = Facts {
            command: &known,
            is_set: &|_| true,
            braces: false,
            glob: false,
        };
        let cls = classify(b"ls {a,b}* ~", None, &facts);
        assert!(cls[3..9].iter().all(|c| c.role == role::ARG));
        assert_eq!(cls[10].role, role::TILDE);
    }

    #[test]
    fn colors() {
        let mut st = crate::style::Styles::default();
        st.set_user("keyword", Style::parse(&["underline"]).unwrap());
        st.set_user("command", Style::parse(&["plain"]).unwrap());
        st.set_user("error", Style::parse(&["bg:red"]).unwrap());
        let r = st.resolver(Some("default-dark"));
        let c = Colors::new(|n| r.get(n));
        assert_eq!(c.sgr(role::KEYWORD), b"4");
        assert_eq!(c.sgr(role::EXTERNAL), b"");
        assert_eq!(c.sgr(role::UNKNOWN), b"1;31");
        assert_eq!(c.sgr(role::FUNCTION), b"1;32");
        assert_eq!(c.sgr(role::SINGLE), b"33");
        assert_eq!(c.sgr(role::SELECTED), b"7");
        assert_eq!(c.sgr(Role::NONE), b"");
        let mut cls = cells("if ls", None);
        assert_eq!(render(b"if ls", &cls, &c), b"\x1b[4mif\x1b[0m ls");
        // Modifiers add to the role's style.
        for c in &mut cls[1..] {
            c.mods = 1;
        }
        assert_eq!(
            render(b"if ls", &cls, &c),
            b"\x1b[4mi\x1b[0m\x1b[4;41mf\x1b[0m\x1b[41m ls\x1b[0m"
        );
    }
}
