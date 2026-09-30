//! History expansion (`setopt history.expand`): csh's `!` references to
//! earlier commands, as bash and zsh have them (DEVELOPING.md, History
//! expansion). It runs on each line the editor reads, before the line is
//! parsed, and a line without `!` (or `^` at its start) costs one scan.
//!
//! A reference is an event (`!!`, `!n`, `!-n`, `!str`, `!?str?`, `!#`, or
//! `!{...}` around one), then optionally a word designator (`:n`, `:x-y`,
//! `^`, `$`, `*`, `%`) and modifiers (`:h`, `:t`, `:r`, `:e`, `:s/l/r/`,
//! `:&`, `:g`, `:p`, `:q`, `:x`). `^old^new` at the start of a command is
//! `!!:s^old^new^`.

use super::history::ShellHistory;

/// What is kept from one line to the next: the last substitution (for `:&`
/// and an empty pattern) and the string of the last `!?str?`.
#[derive(Default)]
pub struct Memory {
    subst: Option<(Vec<u8>, Vec<u8>)>,
    search: Option<Vec<u8>>,
}

/// The result of a modifier: the text and where the modifier ends, `None`
/// if there is none, or an error and where it ends.
type Modified = Result<Option<(Vec<u8>, usize)>, (&'static str, usize)>;

/// A line with its history references replaced.
#[derive(Debug, PartialEq)]
pub struct Expansion {
    pub text: Vec<u8>,
    /// `:p`: print the line and add it to the history, without running it.
    pub print: bool,
}

/// Expands the history references in `line`, a line read by the editor.
/// `pending` is the text read before it of the same command, whose quotes
/// and here-documents the line continues. `None` if there are none; an
/// error is the message to report, and the command is then dropped.
pub fn expand(h: &ShellHistory, mem: &mut Memory, pending: &[u8], line: &[u8]) -> Result<Option<Expansion>, String> {
    let quick = pending.is_empty() && line.first() == Some(&b'^');
    if !quick && !line.contains(&b'!') {
        return Ok(None);
    }
    let mut scan = Scanner::default();
    let mut pos = 0;
    while let Some(i) = scan.next_bang(pending, pos) {
        pos = i + 1;
    }
    let mut x = Expander {
        h,
        mem,
        sticky: None,
        print: false,
    };
    // `^old^new` is `!!:s^old^new^`.
    let rewritten;
    let line = if quick {
        rewritten = [b"!!:s", line].concat();
        &rewritten[..]
    } else {
        line
    };
    let mut out = Vec::with_capacity(line.len());
    let mut pos = 0;
    let mut found = false;
    while let Some(i) = scan.next_bang(line, pos) {
        out.extend_from_slice(&line[pos..i]);
        let (text, end) = x.reference(line, i, &out)?;
        out.extend_from_slice(&text);
        pos = end;
        found = true;
    }
    out.extend_from_slice(&line[pos..]);
    Ok(found.then_some(Expansion {
        text: out,
        print: x.print,
    }))
}

/// A quoting context the scanner is in.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ctx {
    /// `'...'`
    Single,
    /// `$'...'`
    Dollar,
    /// `"..."`
    Double,
    /// `` `...` ``
    Back,
    /// `$(...)`, with the depth of the parentheses in it.
    Cmd(u32),
    /// `$((...))`, likewise.
    Arith(u32),
}

/// Follows the quoting of a command's text to find the `!` that start
/// history references: not in single quotes, `$'...'`, `$((...))`,
/// comments or here-documents, nor after a backslash.
#[derive(Default)]
struct Scanner {
    /// The contexts the text is in, innermost last; empty at the top level.
    ctx: Vec<Ctx>,
    /// The delimiters of the here-documents whose bodies come next, and
    /// whether their leading tabs are stripped (`<<-`).
    heredocs: Vec<(Vec<u8>, bool)>,
    /// Reading here-document bodies.
    body: bool,
}

/// The operators that end words.
fn is_op(c: u8) -> bool {
    matches!(c, b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')')
}

impl Scanner {
    /// Scans `s` from `i` to the next `!` that starts a history reference,
    /// and returns its index, leaving the state as it is there. Once the
    /// reference is replaced, scanning goes on after it.
    fn next_bang(&mut self, s: &[u8], mut i: usize) -> Option<usize> {
        while i < s.len() {
            if self.body {
                i = self.body_line(s, i);
                continue;
            }
            let c = s[i];
            match self.ctx.last().copied() {
                Some(Ctx::Single) => {
                    if c == b'\'' {
                        self.ctx.pop();
                    }
                    i += 1;
                }
                Some(Ctx::Dollar) => match c {
                    b'\\' => i += 2,
                    b'\'' => {
                        self.ctx.pop();
                        i += 1;
                    }
                    _ => i += 1,
                },
                Some(Ctx::Double) => match c {
                    b'\\' => i += 2,
                    b'"' => {
                        self.ctx.pop();
                        i += 1;
                    }
                    b'`' => {
                        self.ctx.push(Ctx::Back);
                        i += 1;
                    }
                    b'$' => i = self.dollar(s, i, false),
                    b'!' if starts_reference(s, i) => return Some(i),
                    _ => i += 1,
                },
                Some(Ctx::Arith(depth)) => {
                    match c {
                        b'(' => self.set_depth(depth + 1),
                        b')' if depth > 0 => self.set_depth(depth - 1),
                        b')' if s.get(i + 1) == Some(&b')') => {
                            self.ctx.pop();
                            i += 1;
                        }
                        _ => {}
                    }
                    i += 1;
                }
                ctx @ (None | Some(Ctx::Back | Ctx::Cmd(_))) => match c {
                    b'\\' => i += 2,
                    b'\'' => {
                        self.ctx.push(Ctx::Single);
                        i += 1;
                    }
                    b'"' => {
                        self.ctx.push(Ctx::Double);
                        i += 1;
                    }
                    b'`' => {
                        if ctx == Some(Ctx::Back) {
                            self.ctx.pop();
                        } else {
                            self.ctx.push(Ctx::Back);
                        }
                        i += 1;
                    }
                    b'$' => i = self.dollar(s, i, true),
                    b'(' | b')' if matches!(ctx, Some(Ctx::Cmd(_))) => {
                        match (c, ctx) {
                            (b'(', Some(Ctx::Cmd(depth))) => self.set_depth(depth + 1),
                            (_, Some(Ctx::Cmd(0))) => {
                                self.ctx.pop();
                            }
                            (_, Some(Ctx::Cmd(depth))) => self.set_depth(depth - 1),
                            _ => {}
                        }
                        i += 1;
                    }
                    b'#' if i == 0 || s[i - 1].is_ascii_whitespace() || is_op(s[i - 1]) => {
                        i = s[i..].iter().position(|&c| c == b'\n').map_or(s.len(), |n| i + n);
                    }
                    b'<' if s.get(i + 1) == Some(&b'<') => i = self.heredoc(s, i + 2),
                    b'\n' => {
                        self.body = !self.heredocs.is_empty();
                        i += 1;
                    }
                    b'!' if starts_reference(s, i) => return Some(i),
                    _ => i += 1,
                },
            }
        }
        None
    }

    /// Sets the depth of the innermost `$(...)` or `$((...))`.
    fn set_depth(&mut self, depth: u32) {
        if let Some(Ctx::Cmd(d) | Ctx::Arith(d)) = self.ctx.last_mut() {
            *d = depth;
        }
    }

    /// After a `$` at `i`: enters `$(`, `$((` or (in code) `$'`.
    fn dollar(&mut self, s: &[u8], i: usize, code: bool) -> usize {
        match (s.get(i + 1), s.get(i + 2)) {
            (Some(b'('), Some(b'(')) => {
                self.ctx.push(Ctx::Arith(0));
                i + 3
            }
            (Some(b'('), _) => {
                self.ctx.push(Ctx::Cmd(0));
                i + 2
            }
            (Some(b'\''), _) if code => {
                self.ctx.push(Ctx::Dollar);
                i + 2
            }
            _ => i + 1,
        }
    }

    /// After `<<` at `i`: records the here-document's delimiter, without
    /// its quotes, and returns where it ends.
    fn heredoc(&mut self, s: &[u8], mut i: usize) -> usize {
        if s.get(i) == Some(&b'<') {
            // `<<<`, a here-string.
            return i + 1;
        }
        let strip = s.get(i) == Some(&b'-');
        i += usize::from(strip);
        while matches!(s.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        let mut delim = Vec::new();
        let mut quote = None;
        while let Some(&c) = s.get(i) {
            match (quote, c) {
                (None, b'\'' | b'"') => quote = Some(c),
                (Some(q), _) if c == q => quote = None,
                (None | Some(b'"'), b'\\') if i + 1 < s.len() => {
                    i += 1;
                    delim.push(s[i]);
                }
                (None, _) if c.is_ascii_whitespace() || is_op(c) => break,
                _ => delim.push(c),
            }
            i += 1;
        }
        if !delim.is_empty() {
            self.heredocs.push((delim, strip));
        }
        i
    }

    /// Skips a line of a here-document's body at `i`, which may be its
    /// delimiter.
    fn body_line(&mut self, s: &[u8], i: usize) -> usize {
        let end = s[i..].iter().position(|&c| c == b'\n').map_or(s.len(), |n| i + n);
        let (delim, strip) = &self.heredocs[0];
        let mut line = &s[i..end];
        if *strip {
            while let [b'\t', rest @ ..] = line {
                line = rest;
            }
        }
        if line == &delim[..] {
            self.heredocs.remove(0);
            self.body = !self.heredocs.is_empty();
        }
        end + 1
    }
}

/// Whether the `!` at `i` starts a history reference: not before a blank,
/// `=` or `(` (as in bash and zsh), `"` (bash), or an operator, and not in
/// `$!`, `${!name}`, `${#!}` or `[!...]` (as in bash).
fn starts_reference(s: &[u8], i: usize) -> bool {
    let literal_after = match s.get(i + 1) {
        None => true,
        Some(&c) => c.is_ascii_whitespace() || is_op(c) || matches!(c, b'=' | b'"' | b'\'' | b'}'),
    };
    let before = &s[..i];
    !(literal_after
        || before.ends_with(b"$")
        || before.ends_with(b"[")
        || before.ends_with(b"${")
        || before.ends_with(b"${#"))
}

/// An event, as a reference names it.
enum Event {
    /// An entry of the history, by event number.
    Number(usize),
    /// The line so far (`!#`).
    Line,
}

struct Expander<'a> {
    h: &'a ShellHistory,
    mem: &'a mut Memory,
    /// The event of the last reference in the line, which a `!` alone
    /// (before a word designator or modifier) refers to, as in zsh.
    sticky: Option<usize>,
    print: bool,
}

/// The characters that end the string of `!str`.
fn ends_string(c: u8) -> bool {
    c.is_ascii_whitespace() || is_op(c) || matches!(c, b':' | b'\'' | b'"' | b'`' | b'\\' | b'}' | b'^' | b'$' | b'*')
}

impl Expander<'_> {
    /// Replaces the reference at `i` (at a `!`) in `s`. `out` is the line
    /// so far, for `!#`. Returns the text and where the reference ends.
    fn reference(&mut self, s: &[u8], i: usize, out: &[u8]) -> Result<(Vec<u8>, usize), String> {
        if s.get(i + 1) == Some(&b'{') {
            let close = s[i..].iter().position(|&c| c == b'}').map(|n| i + n);
            let Some(close) = close else {
                let rest = s[i..].strip_suffix(b"\n").unwrap_or(&s[i..]);
                return Err(format!("{}: event not found", String::from_utf8_lossy(rest)));
            };
            let inner = [b"!", &s[i + 2..close]].concat();
            let (text, end) = self.reference(&inner, 0, out)?;
            if end != inner.len() {
                return Err(format!(
                    "{}: bad word specifier",
                    String::from_utf8_lossy(&s[i..=close])
                ));
            }
            return Ok((text, close + 1));
        }
        let (event, mut j) = self.event(s, i)?;
        let text: Vec<u8> = match event {
            Event::Number(n) => {
                self.sticky = Some(n);
                self.h.event(n).unwrap_or_default().as_bytes().to_vec()
            }
            Event::Line => out.to_vec(),
        };
        let quoted = |j: usize| String::from_utf8_lossy(s[i..j].strip_suffix(b"\n").unwrap_or(&s[i..j])).into_owned();
        let mut text = match self.designator(s, j, &text) {
            Ok(Some((words, end))) => {
                j = end;
                words
            }
            Ok(None) => text,
            Err(end) => return Err(format!("{}: bad word specifier", quoted(end))),
        };
        while s.get(j) == Some(&b':') {
            match self.modifier(s, j + 1, &text) {
                Ok(Some((t, end))) => {
                    text = t;
                    j = end;
                }
                Ok(None) => break,
                Err((msg, end)) => return Err(format!("{}: {msg}", quoted(end))),
            }
        }
        Ok((text, j))
    }

    /// The event of the reference at `i`, and where it ends.
    fn event(&mut self, s: &[u8], i: usize) -> Result<(Event, usize), String> {
        let newest = self
            .h
            .next_event()
            .checked_sub(1)
            .filter(|&n| self.h.event(n).is_some());
        let not_found = |end: usize| format!("{}: event not found", String::from_utf8_lossy(&s[i..end]));
        let j = i + 1;
        let found = |n: Option<usize>, end: usize| match n {
            Some(n) => Ok((Event::Number(n), end)),
            None => Err(not_found(end)),
        };
        match s[j] {
            b'!' => found(newest, j + 1),
            b'#' => Ok((Event::Line, j + 1)),
            b'0'..=b'9' | b'-' if s.get(j + usize::from(s[j] == b'-')).is_some_and(u8::is_ascii_digit) => {
                let start = j + usize::from(s[j] == b'-');
                let end = start + s[start..].iter().take_while(|c| c.is_ascii_digit()).count();
                let n: Option<usize> = std::str::from_utf8(&s[start..end]).ok().and_then(|n| n.parse().ok());
                let n = match (s[j], n) {
                    (b'-', Some(back)) => self.h.next_event().checked_sub(back),
                    (_, n) => n,
                };
                found(n.filter(|&n| self.h.event(n).is_some()), end)
            }
            b'?' => {
                let start = j + 1;
                let len = s[start..].iter().position(|&c| c == b'?' || c == b'\n');
                let stop = len.map_or(s.len(), |n| start + n);
                let end = stop + usize::from(s.get(stop) == Some(&b'?'));
                let term = &s[start..stop];
                if term.is_empty() {
                    return Err(not_found(end));
                }
                self.mem.search = Some(term.to_vec());
                found(self.find(|e| contains(e, term)), end)
            }
            b':' | b'^' | b'$' | b'*' | b'%' => found(self.sticky.or(newest), j),
            _ => {
                let end = j + s[j..].iter().take_while(|&&c| !ends_string(c)).count();
                let prefix = &s[j..end];
                found(self.find(|e| e.starts_with(prefix)), end)
            }
        }
    }

    /// The newest entry of the history for which `f` is true.
    fn find(&self, f: impl Fn(&[u8]) -> bool) -> Option<usize> {
        let (first, last) = (self.h.event_at(0), self.h.next_event());
        (first..last)
            .rev()
            .find(|&n| self.h.event(n).is_some_and(|e| f(e.as_bytes())))
    }

    /// The words of `event` chosen by a word designator at `j`, if there is
    /// one, with where it ends: from the first to the last, as they are in
    /// the event. The error is where the designator ends.
    fn designator(&self, s: &[u8], j: usize, event: &[u8]) -> Result<Option<(Vec<u8>, usize)>, usize> {
        let is_start = |c: Option<&u8>| matches!(c, Some(b'0'..=b'9' | b'^' | b'$' | b'*' | b'%'));
        let mut k = match s.get(j) {
            Some(b':') if is_start(s.get(j + 1)) || s.get(j + 1) == Some(&b'-') => j + 1,
            Some(b'^' | b'$' | b'*' | b'%') => j,
            Some(b'-') if is_start(s.get(j + 1)) => j,
            _ => return Ok(None),
        };
        let words = words(event);
        let last = words.len().checked_sub(1);
        // A word's index: `n`, `^` (1), `$` (the last) or `%` (the word
        // the last `!?str?` found).
        let index = |k: &mut usize| -> Option<Option<usize>> {
            match s.get(*k)? {
                b'0'..=b'9' => {
                    let end = *k + s[*k..].iter().take_while(|c| c.is_ascii_digit()).count();
                    let n = std::str::from_utf8(&s[*k..end]).ok()?.parse().ok();
                    *k = end;
                    Some(n)
                }
                b'^' => {
                    *k += 1;
                    Some(Some(1))
                }
                b'$' => {
                    *k += 1;
                    Some(last)
                }
                b'%' => {
                    *k += 1;
                    let term = self.mem.search.as_deref()?;
                    Some(words.iter().position(|&(a, b)| contains(&event[a..b], term)))
                }
                _ => None,
            }
        };
        let (from, to) = if s[k] == b'*' {
            k += 1;
            (Some(1), last)
        } else if s[k] == b'-' {
            k += 1;
            (Some(0), index(&mut k).ok_or(k)?)
        } else {
            let from = index(&mut k).ok_or(k)?;
            match s.get(k) {
                Some(b'*') => {
                    k += 1;
                    (from, last)
                }
                Some(b'-') => {
                    k += 1;
                    let mut after = k;
                    match index(&mut after) {
                        Some(to) => {
                            k = after;
                            (from, to)
                        }
                        // `x-` leaves out the last word.
                        None => (from, last.and_then(|l| l.checked_sub(1))),
                    }
                }
                _ => (from, from),
            }
        };
        let (Some(from), Some(to)) = (from, to) else {
            return Err(k);
        };
        // A range past the last word is empty (as `*` is for a command
        // alone), but a word that isn't there is an error.
        if from > to {
            return if from <= words.len() {
                Ok(Some((Vec::new(), k)))
            } else {
                Err(k)
            };
        }
        if to >= words.len() {
            return Err(k);
        }
        Ok(Some((event[words[from].0..words[to].1].to_vec(), k)))
    }

    /// Applies the modifier at `j` (after its `:`) to `text`. `None` if
    /// there is none there, which ends the reference before the `:`.
    fn modifier(&mut self, s: &[u8], j: usize, text: &[u8]) -> Modified {
        let global = s.get(j) == Some(&b'g') && matches!(s.get(j + 1), Some(b's' | b'&'));
        let k = j + usize::from(global);
        let Some(&m) = s.get(k) else { return Ok(None) };
        let slash = text.iter().rposition(|&c| c == b'/');
        let dot = text
            .iter()
            .rposition(|&c| c == b'.')
            .filter(|&d| slash.is_none_or(|sl| d > sl));
        let text = match m {
            b'h' => match slash {
                Some(0) => b"/".to_vec(),
                Some(sl) => text[..sl].to_vec(),
                None => text.to_vec(),
            },
            b't' => match slash {
                Some(sl) => text[sl + 1..].to_vec(),
                None => text.to_vec(),
            },
            b'r' => match dot {
                Some(d) => text[..d].to_vec(),
                None => text.to_vec(),
            },
            b'e' => match dot {
                Some(d) => text[d + 1..].to_vec(),
                None => text.to_vec(),
            },
            b'p' => {
                self.print = true;
                text.to_vec()
            }
            b'q' => quote(text),
            b'x' => {
                let words = text.split(u8::is_ascii_whitespace).filter(|w| !w.is_empty());
                words.map(quote).collect::<Vec<_>>().join(&b' ')
            }
            b's' => {
                let (from, to, end) = self.substitution(s, k + 1).map_err(|e| (e, s.len()))?;
                return match replace(text, &from, &to, global) {
                    Some(t) => Ok(Some((t, end))),
                    None => Err(("substitution failed", end)),
                };
            }
            b'&' => {
                let Some((from, to)) = &self.mem.subst else {
                    return Err(("no previous substitution", k + 1));
                };
                return match replace(text, from, to, global) {
                    Some(t) => Ok(Some((t, k + 1))),
                    None => Err(("substitution failed", k + 1)),
                };
            }
            _ => return Ok(None),
        };
        Ok(Some((text, k + 1)))
    }

    /// Reads `/old/new/` at `j` (with any delimiter), and remembers it.
    /// In the new text `&` is the old; a backslash quotes the delimiter
    /// or `&`. The last delimiter may be left out at the end of the line.
    fn substitution(&mut self, s: &[u8], j: usize) -> Result<(Vec<u8>, Vec<u8>, usize), &'static str> {
        let delim = match s.get(j) {
            Some(&c) if c != b'\n' => c,
            _ => return Err("no previous substitution"),
        };
        let part = |k: &mut usize, amp: Option<&[u8]>| {
            let mut t = Vec::new();
            while let Some(&c) = s.get(*k) {
                *k += 1;
                match (c, amp) {
                    (b'\\', _) if matches!(s.get(*k), Some(&n) if n == delim || n == b'&') => {
                        t.push(s[*k]);
                        *k += 1;
                    }
                    (b'\n', _) => {
                        *k -= 1;
                        break;
                    }
                    _ if c == delim => break,
                    (b'&', Some(amp)) => t.extend_from_slice(amp),
                    _ => t.push(c),
                }
            }
            t
        };
        let mut k = j + 1;
        let mut from = part(&mut k, None);
        if from.is_empty() {
            from = match (&self.mem.subst, &self.mem.search) {
                (Some((f, _)), _) => f.clone(),
                (None, Some(term)) => term.clone(),
                (None, None) => return Err("no previous substitution"),
            };
        }
        let to = part(&mut k, Some(&from));
        self.mem.subst = Some((from.clone(), to.clone()));
        Ok((from, to, k))
    }
}

/// Replaces the first `from` in `text` by `to`, or each with `global`.
/// `None` if there is none.
fn replace(text: &[u8], from: &[u8], to: &[u8], global: bool) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut found = false;
    while let Some(p) = find(rest, from) {
        found = true;
        out.extend_from_slice(&rest[..p]);
        out.extend_from_slice(to);
        rest = &rest[p + from.len()..];
        if !global || from.is_empty() {
            break;
        }
    }
    out.extend_from_slice(rest);
    found.then_some(out)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len().max(1)).position(|w| w == needle)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle).is_some()
}

/// `text` in single quotes (`:q`).
fn quote(text: &[u8]) -> Vec<u8> {
    let mut q = vec![b'\''];
    for &c in text {
        if c == b'\'' {
            q.extend_from_slice(b"'\\''");
        } else {
            q.push(c);
        }
    }
    q.push(b'\'');
    q
}

/// The words of a command, for word designators, as (start, end): split
/// at blanks, with quotes, `$(...)` and `${...}` kept whole, and operators
/// as words of their own. A number before `<` or `>` and a file
/// descriptor after `<&` or `>&` belong to the redirection (`2>&1`).
fn words(s: &[u8]) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if is_op(s[i]) {
            i = operator_end(s, i);
        } else {
            i = word_end(s, i);
            if s[start..i].iter().all(u8::is_ascii_digit) && matches!(s.get(i), Some(b'<' | b'>')) {
                i = operator_end(s, i);
            }
        }
        words.push((start, i));
    }
    words
}

fn operator_end(s: &[u8], i: usize) -> usize {
    if matches!(s[i], b'(' | b')') {
        return i + 1;
    }
    let mut j = i + s[i..]
        .iter()
        .take_while(|&&c| is_op(c) && c != b'(' && c != b')')
        .count();
    if s[i..j].ends_with(b">&") || s[i..j].ends_with(b"<&") {
        j += s[j..].iter().take_while(|&&c| c.is_ascii_digit() || c == b'-').count();
    }
    j
}

fn word_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() {
        match s[i] {
            c if c.is_ascii_whitespace() || is_op(c) => break,
            b'\\' => i += 2,
            b'\'' => i = skip_to(s, i + 1, b'\''),
            b'"' | b'`' => i = skip_quoted(s, i + 1, s[i]),
            b'$' if matches!(s.get(i + 1), Some(b'(' | b'{')) => i = skip_balanced(s, i + 1),
            _ => i += 1,
        }
    }
    i.min(s.len())
}

/// The index after the next `c` from `i`.
fn skip_to(s: &[u8], i: usize, c: u8) -> usize {
    s[i.min(s.len())..]
        .iter()
        .position(|&x| x == c)
        .map_or(s.len(), |n| i + n + 1)
}

/// The index after the `close` that ends a string from `i`, in which a
/// backslash quotes the next character.
fn skip_quoted(s: &[u8], mut i: usize, close: u8) -> usize {
    while i < s.len() {
        match s[i] {
            b'\\' => i += 2,
            c if c == close => return i + 1,
            _ => i += 1,
        }
    }
    s.len()
}

/// The index after the bracket that closes the one at `i`, skipping quotes.
fn skip_balanced(s: &[u8], mut i: usize) -> usize {
    let (open, close) = (s[i], if s[i] == b'(' { b')' } else { b'}' });
    let mut depth = 0;
    while i < s.len() {
        match s[i] {
            b'\\' => i += 1,
            b'\'' => i = skip_to(s, i + 1, b'\'') - 1,
            b'"' | b'`' => i = skip_quoted(s, i + 1, s[i]) - 1,
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    s.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(lines: &[&str]) -> ShellHistory {
        let mut h = ShellHistory::default();
        for l in lines {
            h.add_current(l, false);
        }
        h
    }

    /// Expands `line` (without its newline) after the commands `past`.
    fn bang(past: &[&str], line: &str) -> Result<String, String> {
        bang_with(&history(past), &mut Memory::default(), "", line)
    }

    fn bang_with(h: &ShellHistory, mem: &mut Memory, pending: &str, line: &str) -> Result<String, String> {
        let line = format!("{line}\n");
        match expand(h, mem, pending.as_bytes(), line.as_bytes())? {
            Some(x) => {
                let text = String::from_utf8(x.text).unwrap();
                let text = text.strip_suffix('\n').unwrap().to_owned();
                Ok(if x.print { format!("{text} [p]") } else { text })
            }
            None => Ok(line.trim_end_matches('\n').to_owned() + " [unchanged]"),
        }
    }

    const PAST: &[&str] = &["ls /usr/share/doc.d", "echo one two three", "git log -p"];

    #[test]
    fn events() {
        assert_eq!(bang(PAST, "!!").unwrap(), "git log -p");
        assert_eq!(bang(PAST, "sudo !!").unwrap(), "sudo git log -p");
        assert_eq!(bang(PAST, "!-2").unwrap(), "echo one two three");
        assert_eq!(bang(PAST, "!1").unwrap(), "ls /usr/share/doc.d");
        assert_eq!(bang(PAST, "!ec x").unwrap(), "echo one two three x");
        assert_eq!(bang(PAST, "!?two?:0").unwrap(), "echo");
        assert_eq!(bang(PAST, "x!?two").unwrap(), "xecho one two three");
        assert_eq!(bang(PAST, "!{ec}x").unwrap(), "echo one two threex");
        assert_eq!(bang(PAST, "!{!}").unwrap(), "git log -p");
        assert_eq!(bang(PAST, "echo a !#").unwrap(), "echo a echo a ");
        assert_eq!(bang(PAST, "!!!!").unwrap(), "git log -pgit log -p");
        assert_eq!(bang(&["a"], "!!").unwrap(), "a");
    }

    #[test]
    fn event_errors() {
        assert_eq!(bang(&[], "!!").unwrap_err(), "!!: event not found");
        assert_eq!(bang(PAST, "!nosuch x").unwrap_err(), "!nosuch: event not found");
        assert_eq!(bang(PAST, "!9").unwrap_err(), "!9: event not found");
        assert_eq!(bang(PAST, "!-4").unwrap_err(), "!-4: event not found");
        assert_eq!(bang(PAST, "!?zz?").unwrap_err(), "!?zz?: event not found");
        assert_eq!(bang(PAST, "!{ec").unwrap_err(), "!{ec: event not found");
    }

    #[test]
    fn word_designators() {
        let past = &["echo one two three"];
        assert_eq!(bang(past, "x !$").unwrap(), "x three");
        assert_eq!(bang(past, "x !^").unwrap(), "x one");
        assert_eq!(bang(past, "x !*").unwrap(), "x one two three");
        assert_eq!(bang(past, "x !!:2").unwrap(), "x two");
        assert_eq!(bang(past, "x !!:0").unwrap(), "x echo");
        assert_eq!(bang(past, "x !:1-2").unwrap(), "x one two");
        assert_eq!(bang(past, "x !!-2").unwrap(), "x echo one two");
        assert_eq!(bang(past, "x !:-1").unwrap(), "x echo one");
        assert_eq!(bang(past, "x !:2*").unwrap(), "x two three");
        assert_eq!(bang(past, "x !:1-").unwrap(), "x one two");
        assert_eq!(bang(past, "x !:^-$").unwrap(), "x one two three");
        assert_eq!(bang(&["ls"], "x !* y").unwrap(), "x  y");
        assert_eq!(bang(past, "x !!:4").unwrap_err(), "!!:4: bad word specifier");
        // `!` alone is the event of the reference before it in the line.
        assert_eq!(bang(&["a b", "c d"], "!-2:0 !:1 !$").unwrap(), "a b b");
        // A `:` without a designator or modifier after it is text.
        assert_eq!(bang(past, "scp !$:/tmp").unwrap(), "scp three:/tmp");
        assert_eq!(bang(past, "!?tw?:%").unwrap(), "two");
    }

    #[test]
    fn words_of_commands() {
        let past = &["cmd a>b 2>&1 c&&d|e;f \"g h\"$(i j)k x=$((1 + 2)) 'p q' >>z"];
        let all: Vec<String> = (0..16).map(|n| bang(past, &format!("!!:{n}")).unwrap()).collect();
        assert_eq!(
            all,
            [
                "cmd",
                "a",
                ">",
                "b",
                "2>&1",
                "c",
                "&&",
                "d",
                "|",
                "e",
                ";",
                "f",
                "\"g h\"$(i j)k",
                "x=$((1 + 2))",
                "'p q'",
                ">>"
            ]
        );
        // Ranges keep the text between the words.
        assert_eq!(bang(past, "!!:1-3").unwrap(), "a>b");
        assert_eq!(bang(&["a\nb  c"], "!:1-2").unwrap(), "b  c");
    }

    #[test]
    fn modifiers() {
        let past = &["vi src/lib/main.tar.gz"];
        assert_eq!(bang(past, "cd !$:h").unwrap(), "cd src/lib");
        assert_eq!(bang(past, "!$:t").unwrap(), "main.tar.gz");
        assert_eq!(bang(past, "!$:r").unwrap(), "src/lib/main.tar");
        assert_eq!(bang(past, "!$:r:r").unwrap(), "src/lib/main");
        assert_eq!(bang(past, "!$:e").unwrap(), "gz");
        assert_eq!(bang(past, "!$:t:r:e").unwrap(), "tar");
        assert_eq!(bang(&["vi a"], "!$:h").unwrap(), "a");
        assert_eq!(bang(&["vi /a"], "!$:h").unwrap(), "/");
        assert_eq!(bang(&["vi a/b"], "!$:r").unwrap(), "a/b");
        // Without a `/` or an extension, as in bash (zsh's fail).
        assert_eq!(bang(&["vi a.b/c"], "!$:e").unwrap(), "a.b/c");
        assert_eq!(bang(&["echo it's"], "!!:q").unwrap(), "'echo it'\\''s'");
        assert_eq!(bang(&["echo a  b"], "!!:x").unwrap(), "'echo' 'a' 'b'");
        assert_eq!(bang(past, "!!:p").unwrap(), "vi src/lib/main.tar.gz [p]");
        assert_eq!(bang(past, "!!:zz").unwrap(), "vi src/lib/main.tar.gz:zz");
    }

    #[test]
    fn substitutions() {
        let past = &["echo aa ba"];
        assert_eq!(bang(past, "!!:s/a/X/").unwrap(), "echo Xa ba");
        assert_eq!(bang(past, "!!:gs/a/X/").unwrap(), "echo XX bX");
        assert_eq!(bang(past, "!!:s/a/[&]/").unwrap(), "echo [a]a ba");
        assert_eq!(bang(past, "!!:s/a/\\&/").unwrap(), "echo &a ba");
        assert_eq!(bang(past, "!!:s|a|/|").unwrap(), "echo /a ba");
        assert_eq!(bang(past, "!!:s/a/X").unwrap(), "echo Xa ba");
        assert_eq!(bang(past, "!!:s/a/X/ more").unwrap(), "echo Xa ba more");
        assert_eq!(bang(past, "!!:2:s/a/X/").unwrap(), "bX");
        assert_eq!(bang(past, "!!:s/z/X/").unwrap_err(), "!!:s/z/X/: substitution failed");
        assert_eq!(bang(past, "!!:&").unwrap_err(), "!!:&: no previous substitution");
        // `:&` and an empty pattern repeat the last substitution.
        let h = history(past);
        let mut mem = Memory::default();
        assert_eq!(bang_with(&h, &mut mem, "", "!!:s/a/X/").unwrap(), "echo Xa ba");
        assert_eq!(bang_with(&h, &mut mem, "", "!!:g&").unwrap(), "echo XX bX");
        assert_eq!(bang_with(&h, &mut mem, "", "!!:s//Y/").unwrap(), "echo Ya ba");
    }

    #[test]
    fn quick_substitution() {
        let past = &["make tset", "echo one"];
        assert_eq!(bang(past, "^one^two").unwrap(), "echo two");
        assert_eq!(bang(past, "^one^two^").unwrap(), "echo two");
        assert_eq!(bang(past, "^one^two^ three").unwrap(), "echo two three");
        assert_eq!(bang(past, "^one").unwrap(), "echo ");
        assert_eq!(bang(past, "^zz^two").unwrap_err(), "!!:s^zz^two: substitution failed");
        // Only at the start of a command.
        let (h, mut mem) = (history(past), Memory::default());
        assert_eq!(
            bang_with(&h, &mut mem, "echo \\\n", "^a^b").unwrap(),
            "^a^b [unchanged]"
        );
        assert_eq!(bang(past, "x ^a^b").unwrap(), "x ^a^b [unchanged]");
    }

    #[test]
    fn not_references() {
        let past = &["echo one"];
        for line in [
            "echo hi!",
            "echo a! b",
            "echo 'a!!b'",
            "echo $'a!!b'",
            "echo \\!!",
            "echo \"a\\!!\"",
            "echo \"hi!\"",
            "[ a != b ]",
            "! true",
            "kill $!",
            "echo ${!x} ${#!}",
            "ls [!a]*",
            "echo $((!x))",
            "echo $(( $(echo 1) + !x ))",
            "f() { echo !; }",
            "(echo hi!)",
            "echo hi!; echo hi!&",
            "echo hi!|cat",
            "echo !(x)",
            ": # comment!!",
            "echo a # it's!!",
        ] {
            assert_eq!(bang(past, line).unwrap(), format!("{line} [unchanged]"), "{line}");
        }
    }

    #[test]
    fn quoting_contexts() {
        let past = &["echo one"];
        assert_eq!(bang(past, "echo \"x!!y\"").unwrap(), "echo \"xecho oney\"");
        assert_eq!(bang(past, "echo $(echo !!)").unwrap(), "echo $(echo echo one)");
        assert_eq!(bang(past, "echo `echo !!`").unwrap(), "echo `echo echo one`");
        assert_eq!(
            bang(past, "echo \"$(echo '!!')\" !!").unwrap(),
            "echo \"$(echo '!!')\" echo one"
        );
        assert_eq!(bang(past, "echo $((1)) !!").unwrap(), "echo $((1)) echo one");
        assert_eq!(
            bang(past, "echo $((2*(1+1))) !!").unwrap(),
            "echo $((2*(1+1))) echo one"
        );
        assert_eq!(
            bang(past, "echo $(f(){ :; }) !!").unwrap(),
            "echo $(f(){ :; }) echo one"
        );
        assert_eq!(bang(past, "echo 'a'!!").unwrap(), "echo 'a'echo one");
    }

    #[test]
    fn continuation_lines() {
        let (h, mut mem) = (history(&["echo one"]), Memory::default());
        let mut cont = |pending: &str, line: &str| bang_with(&h, &mut mem, pending, line).unwrap();
        // Quotes opened on an earlier line.
        assert_eq!(cont("echo 'a\n", "!!'"), "!!' [unchanged]");
        assert_eq!(cont("echo 'a\n", "b' !!"), "b' echo one");
        assert_eq!(cont("echo \"a\n", "!!\""), "echo one\"");
        assert_eq!(cont("echo $(\n", "echo '!!'"), "echo '!!' [unchanged]");
        // Here-documents' bodies are left alone, but not what follows them.
        assert_eq!(cont("cat <<E\n", "!!"), "!! [unchanged]");
        assert_eq!(cont("cat <<'E' | cat <<-F\n", "!!"), "!! [unchanged]");
        assert_eq!(cont("cat <<'E' | cat <<-F\nE\n", "\t!!"), "\t!! [unchanged]");
        assert_eq!(cont("cat <<'E' | cat <<-F\nE\n\tF\n", "!!"), "echo one");
        assert_eq!(cont("cat <<E; {\nx\nE\n", "!!"), "echo one");
        assert_eq!(cont("cat <<\"E\"\n", "!!"), "!! [unchanged]");
        assert_eq!(cont("cat <<\\E\n", "!!"), "!! [unchanged]");
    }
}
