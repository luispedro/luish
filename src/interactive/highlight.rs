//! Syntax highlighting for the line editor.
//!
//! Like the completer, the highlighter never touches `Shell`: it uses the
//! `Names` snapshot, plus the text of the command read so far (for `PS2`
//! lines) and the colours, all set before each prompt. It classifies each
//! byte with a rough tokenizer that follows quoting, expansions, operators,
//! redirections, here-documents, comments and reserved words, and colours
//! command names by whether they can be found. A name under the cursor is
//! not marked as unknown, since it may still be being typed.
//!
//! The colours come from `$LUISH_HIGHLIGHT`, a colon-separated list of
//! `class=SGR` entries (as in `GREP_COLORS`) that override the defaults; an
//! empty SGR leaves that class uncoloured. Highlighting is off if the
//! variable is `none` or if `$NO_COLOR` is set and not empty.

use std::borrow::Cow;
use std::collections::HashMap;

use rustyline::highlight::{CmdKind, Highlighter};

use super::complete::{PRECOMMANDS, RESERVED, ShellHelper, is_executable};
use crate::sys;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Plain,
    Keyword,
    Command,
    /// A command name that is not a built-in, function, alias or executable.
    Unknown,
    String,
    Var,
    /// The delimiters of `$(...)` and backquotes.
    Subst,
    Op,
    Redir,
    Comment,
    /// The `NAME=` of an assignment.
    Assign,
    /// Not in the line: the selected match in the completion menu.
    Select,
    /// Not in the line: the descriptions in the completion menu.
    Desc,
    /// Not in the line: an autosuggestion.
    Suggest,
}

const CLASSES: usize = Class::Suggest as usize + 1;

/// Class names in `$LUISH_HIGHLIGHT`, with their default SGR parameters.
const DEFAULTS: &[(&str, Class, &str)] = &[
    ("keyword", Class::Keyword, "1;34"),
    ("command", Class::Command, "32"),
    ("unknown", Class::Unknown, "1;31"),
    ("string", Class::String, "33"),
    ("var", Class::Var, "36"),
    ("subst", Class::Subst, "35"),
    ("op", Class::Op, "1"),
    ("redir", Class::Redir, "1"),
    ("comment", Class::Comment, "90"),
    ("assign", Class::Assign, "34"),
    ("select", Class::Select, "7"),
    ("desc", Class::Desc, "90"),
    ("suggest", Class::Suggest, "90"),
];

/// The SGR parameters (without `ESC [` and `m`) of each class.
#[derive(Debug, PartialEq)]
pub struct Colors([Vec<u8>; CLASSES]);

impl Colors {
    /// Parses `$LUISH_HIGHLIGHT` over the defaults. Returns None if
    /// highlighting is off. Unknown classes and SGR parameters with anything
    /// but digits and `;` are ignored.
    pub fn parse(spec: &[u8]) -> Option<Colors> {
        if spec == b"none" {
            return None;
        }
        let mut c = Colors(Default::default());
        for &(_, class, sgr) in DEFAULTS {
            c.0[class as usize] = sgr.as_bytes().to_vec();
        }
        for entry in spec.split(|&b| b == b':') {
            let Some(eq) = entry.iter().position(|&b| b == b'=') else {
                continue;
            };
            let (name, sgr) = (&entry[..eq], &entry[eq + 1..]);
            if let Some(&(_, class, _)) = DEFAULTS.iter().find(|d| d.0.as_bytes() == name)
                && sgr.iter().all(|&b| b.is_ascii_digit() || b == b';')
            {
                c.0[class as usize] = sgr.to_vec();
            }
        }
        Some(c)
    }

    /// The SGR parameters of `class`.
    pub fn sgr(&self, class: Class) -> &[u8] {
        &self.0[class as usize]
    }
}

/// What the highlighter needs besides `Names`, refreshed before each prompt.
#[derive(Default)]
pub struct State {
    pub colors: Option<Colors>,
    /// The earlier lines of an incomplete command.
    pub context: Vec<u8>,
    /// Whether each command name looked up since the prompt was found.
    pub known: std::cell::RefCell<HashMap<Vec<u8>, bool>>,
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
}

struct Scan<'a> {
    s: &'a [u8],
    cls: Vec<Class>,
    known: &'a dyn Fn(&[u8]) -> bool,
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

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

fn ends_word(c: u8) -> bool {
    b" \t\n;&|()<>".contains(&c)
}

impl Scan<'_> {
    fn paint(&mut self, a: usize, b: usize, c: Class) {
        let b = b.min(self.s.len());
        self.cls[a..b].fill(c);
    }

    /// Paints the bytes in `a..b` not already painted.
    fn paint_plain(&mut self, a: usize, b: usize, c: Class) {
        for x in &mut self.cls[a..b] {
            if *x == Class::Plain {
                *x = c;
            }
        }
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
                    self.paint(i, e, Class::Comment);
                    i = e;
                }
                b';' | b'&' | b'|' => {
                    let len = if s.get(i + 1) == Some(&c) { 2 } else { 1 };
                    self.paint(i, i + len, Class::Op);
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
                    self.paint(i, i + 1, Class::Op);
                    i += 1;
                    let close = i + s[i..].iter().take_while(|&&c| is_blank(c)).count();
                    if !pattern && s.get(close) == Some(&b')') {
                        // A function definition: the body follows.
                        self.paint(close, close + 1, Class::Op);
                        i = close + 1;
                        cmd = true;
                    } else if cmd && !pattern {
                        i = self.list(i, Some(b')'));
                        if s.get(i) == Some(&b')') {
                            self.paint(i, i + 1, Class::Op);
                            i += 1;
                        }
                        cmd = false;
                    }
                }
                b')' => {
                    self.paint(i, i + 1, Class::Op);
                    i += 1;
                    if pattern {
                        pattern = false;
                        cmd = true;
                    }
                }
                b'<' | b'>' => i = self.redirect(i, i),
                b'0'..=b'9' => {
                    let d = i + s[i..].iter().take_while(|c| c.is_ascii_digit()).count();
                    if matches!(s.get(d), Some(b'<' | b'>')) {
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
        if *pattern {
            if plain && text == b"esac" {
                self.paint(start, e, Class::Keyword);
                *pattern = false;
                *cmd = false;
            }
            return e;
        }
        match *after {
            After::For => {
                *after = After::ForName;
                return e;
            }
            After::Case => {
                *after = After::CaseWord;
                return e;
            }
            After::ForName | After::CaseWord if plain && text == b"in" => {
                self.paint(start, e, Class::Keyword);
                *pattern = *after == After::CaseWord;
                *after = After::None;
                return e;
            }
            After::Function if !(plain && text == b"{") => {
                self.paint_plain(start, e, Class::Command);
                return e;
            }
            After::Function => *after = After::None,
            After::ForName if plain && text == b"do" => {
                self.paint(start, e, Class::Keyword);
                *after = After::None;
                *cmd = true;
                return e;
            }
            _ => {}
        }
        if !*cmd || (*precommand && text.starts_with(b"-")) {
            return e;
        }
        let raw = &self.s[start..e];
        if plain && (RESERVED.contains(&text) || text == b"{" || text == b"}" || text == b"!") {
            self.paint(start, e, Class::Keyword);
            match text {
                b"for" => *after = After::For,
                b"case" => *after = After::Case,
                b"function" => *after = After::Function,
                _ => {}
            }
            *cmd = !matches!(text, b"for" | b"case" | b"fi" | b"done" | b"esac" | b"}");
            *precommand = false;
        } else if let Some(eq) = raw.iter().position(|&c| c == b'=')
            && crate::lexer::is_valid_name(&raw[..eq])
        {
            self.paint(start, start + eq + 1, Class::Assign);
        } else if w.expanded {
            *cmd = false;
            *precommand = false;
        } else {
            let rest = &self.s[e..];
            let definition = rest.iter().find(|&&c| !is_blank(c)) == Some(&b'(');
            if definition || (self.known)(text) {
                self.paint_plain(start, e, Class::Command);
            } else if !self.cursor.is_some_and(|c| (start..=e).contains(&c)) {
                self.paint_plain(start, e, Class::Unknown);
            }
            *precommand = PRECOMMANDS.contains(&text);
            *cmd = *precommand;
        }
        e
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
            if ends_word(c) || (c == b'`' && self.in_backquote) {
                break;
            }
            match c {
                b'\\' => {
                    if s.get(j + 1) != Some(&b'\n') {
                        w.quoted = true;
                        w.text.extend(s.get(j + 1));
                    }
                    j += 2;
                }
                b'\'' => {
                    w.quoted = true;
                    let e = self.find(j + 1, b'\'').map_or(s.len(), |e| e + 1);
                    w.text.extend_from_slice(&s[j + 1..e.max(j + 1)]);
                    self.paint(j, e, Class::String);
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
        self.paint(start, start + 1, Class::String);
        let mut k = start + 1;
        while k < s.len() {
            match s[k] {
                b'"' => {
                    self.paint(k, k + 1, Class::String);
                    return k + 1;
                }
                b'\\' => {
                    self.paint(k, k + 2, Class::String);
                    match s.get(k + 1) {
                        Some(&n) if b"$`\"\\\n".contains(&n) => w.text.push(n),
                        _ => w.text.push(b'\\'),
                    }
                    k += 2;
                }
                b'$' => {
                    let e = self.dollar(k);
                    if e == k + 1 {
                        self.paint(k, e, Class::String);
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
                    self.paint(k, k + 1, Class::String);
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
        let e = match (s.get(i + 1), s.get(i + 2)) {
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
                k
            }
            (Some(b'('), _) => {
                self.paint(i, i + 2, Class::Subst);
                let saved = std::mem::replace(&mut self.in_backquote, false);
                let k = self.list(i + 2, Some(b')'));
                self.in_backquote = saved;
                if s.get(k) == Some(&b')') {
                    self.paint(k, k + 1, Class::Subst);
                    return k + 1;
                }
                return k;
            }
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
                k.min(s.len())
            }
            (Some(&c), _) if c.is_ascii_alphabetic() || c == b'_' => {
                i + 1
                    + s[i + 1..]
                        .iter()
                        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                        .count()
            }
            (Some(&c), _) if c.is_ascii_digit() || b"@*#?-$!".contains(&c) => i + 2,
            _ => return i + 1,
        };
        self.paint(i, e, Class::Var);
        e
    }

    fn backquote(&mut self, i: usize) -> usize {
        self.paint(i, i + 1, Class::Subst);
        let saved = std::mem::replace(&mut self.in_backquote, true);
        let k = self.list(i + 1, Some(b'`'));
        self.in_backquote = saved;
        if self.s.get(k) == Some(&b'`') {
            self.paint(k, k + 1, Class::Subst);
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
        self.paint(start, k, Class::Redir);
        while k < s.len() && is_blank(s[k]) {
            k += 1;
        }
        if k >= s.len() || ends_word(s[k]) {
            return k;
        }
        let w = self.word(k);
        if dup {
            self.paint_plain(k, w.end, Class::Redir);
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
                    self.paint(i, e, Class::Redir);
                    i = e + 1;
                    break;
                }
                self.paint(i, e + 1, Class::String);
                i = e + 1;
            }
        }
        i.min(self.s.len())
    }
}

/// Classifies each byte of `text`. `known` says whether a command name
/// can be found.
pub fn classify(text: &[u8], cursor: Option<usize>, known: &dyn Fn(&[u8]) -> bool) -> Vec<Class> {
    let mut sc = Scan {
        s: text,
        cls: vec![Class::Plain; text.len()],
        known,
        cursor,
        heredocs: Vec::new(),
        in_backquote: false,
    };
    let mut i = 0;
    while i < text.len() {
        // Stray closing parentheses and backquotes.
        i = sc.list(i, None);
        if i < text.len() {
            sc.paint(i, i + 1, Class::Op);
            i += 1;
        }
    }
    sc.cls
}

/// Adds SGR sequences to `line` for the classes `cls`, changing colour only
/// at character boundaries.
pub fn render(line: &[u8], cls: &[Class], colors: &Colors) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 32);
    let mut cur: &[u8] = b"";
    for (&b, &c) in line.iter().zip(cls) {
        if b & 0xc0 != 0x80 {
            let want = &colors.0[c as usize][..];
            if want != cur {
                if !cur.is_empty() {
                    out.extend_from_slice(b"\x1b[0m");
                }
                if !want.is_empty() {
                    out.extend_from_slice(b"\x1b[");
                    out.extend_from_slice(want);
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
    fn is_known(&self, name: &[u8]) -> bool {
        if let Some(&k) = self.highlight.known.borrow().get(name) {
            return k;
        }
        let k = if name.contains(&b'/') {
            match (name.strip_prefix(b"~/"), &self.names.home) {
                (Some(rest), Some(home)) => is_executable(&[&home[..], b"/", rest].concat()),
                _ => is_executable(name),
            }
        } else {
            crate::builtins::names().any(|b| b == name)
                || self.names.functions.iter().any(|c| c == name)
                || self.names.aliases.contains(name)
                || self.names.aliases.for_suffix(name).is_some()
                || self.names.path.split(|&c| c == b':').any(|d| {
                    let d: &[u8] = if d.is_empty() { b"." } else { d };
                    is_executable(&[d, b"/", name].concat())
                })
        };
        let k = k || (self.names.autocd && self.is_autocd_dir(name));
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
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let Some(colors) = &self.highlight.colors else {
            return Cow::Borrowed(line);
        };
        let context = &self.highlight.context;
        let text = [&context[..], line.as_bytes()].concat();
        let cls = classify(&text, Some(context.len() + pos), &|name| self.is_known(name));
        String::from_utf8(render(line.as_bytes(), &cls[context.len()..], colors))
            .map_or(Cow::Borrowed(line), Cow::Owned)
    }

    /// Every change repaints, including cursor moves, since the word under
    /// the cursor is coloured differently.
    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        self.highlight.colors.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One letter per byte: `k`eyword, `c`ommand, `u`nknown, `s`tring,
    /// `v`ar, `$` substitution, `o`perator, `r`edirection, `#` comment,
    /// `a`ssignment, `.` plain.
    fn classes(text: &str) -> String {
        classes_at(text, None)
    }

    fn classes_at(text: &str, cursor: Option<usize>) -> String {
        let known = |n: &[u8]| [&b"echo"[..], b"cat", b"ls", b"sudo"].contains(&n);
        classify(text.as_bytes(), cursor, &known)
            .into_iter()
            .map(|c| match c {
                Class::Plain => '.',
                Class::Keyword => 'k',
                Class::Command => 'c',
                Class::Unknown => 'u',
                Class::String => 's',
                Class::Var => 'v',
                Class::Subst => '$',
                Class::Op => 'o',
                Class::Redir => 'r',
                Class::Comment => '#',
                Class::Assign => 'a',
                Class::Select | Class::Desc | Class::Suggest => unreachable!(),
            })
            .collect()
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
        assert_eq!(classes("! ls"), "k.cc");
        assert_eq!(classes("(ls)"), "occo");
        assert_eq!(classes("e\"ch\"o"), "cssssc");
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
        assert_eq!(classes("echo ${a:-x} $1 $$ $((1+2))"), "cccc.vvvvvvv.vv.vv.vvvvvvvv");
        assert_eq!(classes("echo $(ls -l) `nope`"), "cccc.$$cc...$.$uuuu$");
        assert_eq!(classes("echo \"$(ls \")\")\""), "cccc.s$$cc.sss$s");
        assert_eq!(classes("echo a\\ b $"), "cccc.......");
        assert_eq!(classes("echo x # c"), "cccc...###");
        assert_eq!(classes("echo x#y"), "cccc....");
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
    fn colors() {
        let c = Colors::parse(b"keyword=4:command=:bogus=1:string=1m").unwrap();
        assert_eq!(c.0[Class::Keyword as usize], b"4");
        assert_eq!(c.0[Class::Command as usize], b"");
        assert_eq!(c.0[Class::String as usize], b"33");
        assert_eq!(Colors::parse(b"none"), None);
        let cls = classify(b"if ls", None, &|_| true);
        assert_eq!(render(b"if ls", &cls, &c), b"\x1b[4mif\x1b[0m ls");
    }
}
