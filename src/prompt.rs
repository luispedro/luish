//! zsh's prompt expansion: the `%` sequences in `PS1`, `PS2` and `PS4`,
//! expanded when the `prompt.percent` option is on, after parameter
//! expansion (as zsh does with `PROMPT_SUBST`).
//!
//! The result keeps the escape sequences (colours, `%{...%}`) apart from
//! the visible text, so that the line editor can measure the prompt's width
//! without them.

use crate::shell::Shell;
use crate::style::{self, Color, Style};
use crate::sys;

/// An expanded prompt.
#[derive(Clone)]
pub struct Prompt {
    /// What to write to the terminal.
    pub text: Vec<u8>,
    /// The same without escape sequences, if it differs: what the line
    /// editor measures.
    pub plain: Option<Vec<u8>>,
}

impl Prompt {
    pub fn plain(text: Vec<u8>) -> Prompt {
        Prompt { text, plain: None }
    }
}

/// A piece of the expanded prompt.
enum Part {
    /// Visible text.
    Text(Vec<u8>),
    /// An escape sequence, which takes no room on the screen.
    Escape(Vec<u8>),
    /// `%nG`: the escape sequences around it take `n` columns (at most
    /// `MAX_GLITCH`).
    Glitch(usize),
}

/// More columns than any terminal has.
const MAX_GLITCH: usize = 1 << 16;

/// A pending truncation (`%n<str<` or `%n>str>`): it applies to the parts
/// from `start` to the end of the group or the next truncation.
struct Trunc {
    start: usize,
    len: usize,
    marker: Vec<u8>,
    /// `<`: remove text on the left.
    left: bool,
}

/// Expands the `%` sequences of `s`.
pub fn expand(sh: &Shell, s: &[u8]) -> Prompt {
    let mut e = Expander {
        sh,
        s,
        pos: 0,
        in_escape: false,
        tm: None,
        sgr: Sgr::default(),
        styles: None,
    };
    let parts = e.group(None);
    let mut text = Vec::new();
    let mut plain = Vec::new();
    let mut escapes = false;
    for p in parts {
        match p {
            Part::Text(t) => {
                text.extend_from_slice(&t);
                plain.extend_from_slice(&t);
            }
            Part::Escape(t) => {
                text.extend_from_slice(&t);
                escapes = true;
            }
            Part::Glitch(n) => {
                plain.resize(plain.len() + n, b' ');
                escapes = true;
            }
        }
    }
    Prompt {
        text,
        plain: escapes.then_some(plain),
    }
}

struct Expander<'a> {
    sh: &'a Shell,
    s: &'a [u8],
    pos: usize,
    /// Inside `%{...%}`: all output is an escape sequence.
    in_escape: bool,
    /// The time, found when first needed.
    tm: Option<libc::tm>,
    /// The attributes and colours in effect, for `%[style_off]`.
    sgr: Sgr,
    /// The styles, resolved when first needed; None inside with
    /// `$NO_COLOR`.
    styles: Option<Option<crate::style::Resolver<'a>>>,
}

/// The attributes and colours that the sequences so far have set (but not
/// those of `%{...%}`), and those to go back to at each `%[style_off]`.
#[derive(Clone, Default)]
struct Sgr {
    cur: Style,
    stack: Vec<Style>,
}

/// Internal letters of `%[style:NAME]` and `%[style_off]`, which have no
/// short form.
const STYLE: u8 = 1;
const STYLE_OFF: u8 = 2;

impl<'a> Expander<'a> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn push(&self, parts: &mut Vec<Part>, text: impl Into<Vec<u8>>) {
        let text = text.into();
        if self.in_escape {
            parts.push(Part::Escape(text));
        } else if !text.is_empty() {
            parts.push(Part::Text(text));
        }
    }

    /// Expands up to the end of the string, or up to `end` (which is
    /// consumed): a group of `%(...)`.
    fn group(&mut self, end: Option<u8>) -> Vec<Part> {
        let mut parts = Vec::new();
        let mut trunc: Option<Trunc> = None;
        while let Some(c) = self.next() {
            if Some(c) == end {
                break;
            }
            if c != b'%' {
                let start = self.pos - 1;
                while self.peek().is_some_and(|c| c != b'%' && Some(c) != end) {
                    self.pos += 1;
                }
                self.push(&mut parts, &self.s[start..self.pos]);
                continue;
            }
            let mut arg = self.number();
            let Some(mut c) = self.next() else { break };
            // zsh's deprecated `%[N<str]`, the same as `%N<str<`.
            let mut end = c;
            if c == b'[' && self.old_trunc() {
                arg = self.number().or(arg);
                c = self.next().unwrap_or(b'<');
                end = b']';
            }
            match c {
                b'<' | b'>' => {
                    let marker = self.delimited(end);
                    if let Some(t) = trunc.take() {
                        truncate(&mut parts, t);
                    }
                    let len = arg.unwrap_or(0);
                    if len > 0 {
                        trunc = Some(Trunc {
                            start: parts.len(),
                            len: len as usize,
                            marker,
                            left: c == b'<',
                        });
                    }
                }
                b'(' => {
                    let mut arg = arg.or_else(|| self.number());
                    let Some(mut cond) = self.next() else { break };
                    if cond == b'[' {
                        match self.long_name(CONDITIONS, "condition", b"%(") {
                            Some((c, n, _)) => (cond, arg) = (c, n.or(arg)),
                            None => cond = 0,
                        }
                    }
                    let sep = self.next();
                    // Both branches are expanded, each from the attributes
                    // in effect before them; the one taken sets them.
                    let before = self.sgr.clone();
                    let yes = self.group(sep);
                    let after_yes = std::mem::replace(&mut self.sgr, before);
                    let no = self.group(Some(b')'));
                    // An unknown condition expands to nothing.
                    if cond != 0 {
                        if self.test(cond, arg.unwrap_or(0)) {
                            self.sgr = after_yes;
                            parts.extend(yes);
                        } else {
                            parts.extend(no);
                        }
                    }
                }
                b'[' => {
                    if let Some((c, n, brace)) = self.long_name(SEQUENCES, "sequence", b"%") {
                        self.escape(c, n.or(arg), brace, &mut parts);
                    }
                }
                // The letters of sequences with only a long name.
                c if c.is_ascii_control() => {}
                _ => {
                    let brace = if takes_brace(c) { self.braced() } else { None };
                    self.escape(c, arg, brace, &mut parts);
                }
            }
        }
        if let Some(t) = trunc {
            truncate(&mut parts, t);
        }
        parts
    }

    /// An optional integer argument, as in `%2~` or `%-1/`.
    fn number(&mut self) -> Option<i64> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let n = std::str::from_utf8(&self.s[start..self.pos]).ok()?.parse().ok();
        if n.is_none() {
            self.pos = start;
        }
        n
    }

    /// The text up to `end` (consumed), where a backslash quotes the next
    /// character.
    fn delimited(&mut self, end: u8) -> Vec<u8> {
        self.delimited_closed(end).0
    }

    /// The same, and whether `end` was found.
    fn delimited_closed(&mut self, end: u8) -> (Vec<u8>, bool) {
        let mut out = Vec::new();
        while let Some(c) = self.next() {
            match c {
                b'\\' => out.extend(self.next()),
                c if c == end => return (out, true),
                c => out.push(c),
            }
        }
        (out, false)
    }

    /// After `%[`: whether this is zsh's `%[N<str]` (digits, then `<` or
    /// `>`) rather than a long name.
    fn old_trunc(&self) -> bool {
        let rest = &self.s[self.pos..];
        let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        matches!(rest.get(digits), Some(b'<' | b'>'))
    }

    /// After `[`: a long name, `name]` or `name:arg]`, looked up in `table`.
    /// Returns its letter and its argument: a number, or the text of a
    /// `{...}` argument for the sequences that take one. Otherwise reports
    /// an error (`what` and `intro` say what was being read, for the
    /// message) and returns `None`.
    fn long_name(
        &mut self,
        table: &[(&str, u8)],
        what: &str,
        intro: &[u8],
    ) -> Option<(u8, Option<i64>, Option<Vec<u8>>)> {
        let (inner, closed) = self.delimited_closed(b']');
        let seq = |name: &[u8]| [intro, b"[", name, b"]"].concat();
        if !closed {
            let seq = [intro, b"[", &inner].concat();
            self.sh
                .error([b"missing ] in prompt ", what.as_bytes(), b" ", &seq].concat());
            return None;
        }
        let (name, sub) = match inner.iter().position(|&c| c == b':') {
            Some(i) => (&inner[..i], Some(&inner[i + 1..])),
            None => (&inner[..], None),
        };
        let key = name_key(name);
        if let Some(&(_, c)) = table.iter().find(|(n, _)| name_key(n.as_bytes()) == key) {
            return match sub {
                None => Some((c, None, None)),
                Some(sub) if what == "sequence" && takes_brace(c) => Some((c, None, Some(sub.to_vec()))),
                Some(sub) => match std::str::from_utf8(sub).ok().and_then(|s| s.parse().ok()) {
                    Some(n) => Some((c, Some(n), None)),
                    None => {
                        let msg = [b"Illegal number in prompt ", what.as_bytes(), b" ", &seq(&inner)].concat();
                        self.sh.error(msg);
                        None
                    }
                },
            };
        }
        let mut msg = [b"unknown prompt ", what.as_bytes(), b" ", &seq(name)].concat();
        match suggest(table, &key) {
            Some(n) => {
                msg.extend_from_slice(b"; did you mean ");
                msg.extend_from_slice(&seq(n.as_bytes()));
                msg.push(b'?');
            }
            None => {
                msg.extend_from_slice(b"; the names are ");
                let names: Vec<&str> = table.iter().map(|(n, _)| *n).collect();
                msg.extend_from_slice(names.join(", ").as_bytes());
            }
        }
        self.sh.error(msg);
        None
    }

    /// The text of a `{...}` argument, if there is one.
    fn braced(&mut self) -> Option<Vec<u8>> {
        if self.peek() != Some(b'{') {
            return None;
        }
        self.pos += 1;
        Some(self.delimited(b'}'))
    }

    fn time(&mut self) -> &libc::tm {
        self.tm.get_or_insert_with(sys::localtime)
    }

    fn strftime(&mut self, fmt: &[u8]) -> Vec<u8> {
        // zsh's own formats: the day, and hours, without padding.
        let mut f = Vec::with_capacity(fmt.len());
        let mut it = fmt.iter();
        while let Some(&c) = it.next() {
            f.push(c);
            if c == b'%' {
                match it.next() {
                    Some(b'f') => f.extend_from_slice(b"-d"),
                    Some(b'K') => f.extend_from_slice(b"-H"),
                    Some(b'L') => f.extend_from_slice(b"-I"),
                    Some(&c) => f.push(c),
                    None => {}
                }
            }
        }
        sys::strftime(&f, self.time())
    }

    /// `%c` for a character `c` other than `(`, `<` and `>`, with its
    /// numeric argument and, for those that take one, its `{...}` argument.
    fn escape(&mut self, c: u8, arg: Option<i64>, brace: Option<Vec<u8>>, parts: &mut Vec<Part>) {
        let sh = self.sh;
        let text: Vec<u8> = match c {
            b'%' | b')' => vec![c],
            b'~' => dir(sh, arg.unwrap_or(0), true),
            b'd' | b'/' => dir(sh, arg.unwrap_or(0), false),
            b'c' | b'.' => dir(sh, arg.filter(|&n| n > 0).unwrap_or(1), true),
            b'C' => dir(sh, arg.filter(|&n| n > 0).unwrap_or(1), false),
            b'n' => user_name(),
            b'M' => sys::hostname(),
            b'm' => host_components(&sys::hostname(), arg.unwrap_or(1)),
            b'#' => vec![if sys::geteuid() == 0 { b'#' } else { b'%' }],
            b'?' => sh.last_status.to_string().into_bytes(),
            b'h' | b'!' => history_number().to_string().into_bytes(),
            b'j' => sh.jobs.order().len().to_string().into_bytes(),
            b'L' => shlvl(sh).to_string().into_bytes(),
            b'i' => sh.lineno.to_string().into_bytes(),
            b'l' | b'y' => match sys::ttyname(0) {
                Some(t) => {
                    let t = t.strip_prefix(b"/dev/").unwrap_or(&t);
                    let t = if c == b'l' {
                        t.strip_prefix(b"tty").unwrap_or(t)
                    } else {
                        t
                    };
                    t.to_vec()
                }
                None => b"()".to_vec(),
            },
            b'D' => match brace {
                Some(fmt) => self.strftime(&fmt),
                None => self.strftime(b"%y-%m-%d"),
            },
            b'T' => self.strftime(b"%H:%M"),
            b'*' => self.strftime(b"%H:%M:%S"),
            b't' | b'@' => self.strftime(b"%l:%M%p"),
            b'w' => self.strftime(b"%a %f"),
            b'W' => self.strftime(b"%m/%d/%y"),
            b'B' | b'b' | b'U' | b'u' | b'S' | b's' | b'E' | b'F' | b'f' | b'K' | b'k' => {
                let cur = &mut self.sgr.cur;
                let mut attr = |name, on| match on {
                    true => cur.on |= style::attr_bit(name),
                    false => cur.on &= !style::attr_bit(name),
                };
                let seq: Vec<u8> = match c {
                    b'B' => {
                        attr("bold", true);
                        b"\x1b[1m".to_vec()
                    }
                    b'b' => {
                        // SGR 22 ends dim too.
                        attr("bold", false);
                        attr("dim", false);
                        b"\x1b[22m".to_vec()
                    }
                    b'U' => {
                        attr("underline", true);
                        b"\x1b[4m".to_vec()
                    }
                    b'u' => {
                        attr("underline", false);
                        b"\x1b[24m".to_vec()
                    }
                    b'S' => {
                        attr("reverse", true);
                        b"\x1b[7m".to_vec()
                    }
                    b's' => {
                        attr("reverse", false);
                        b"\x1b[27m".to_vec()
                    }
                    b'E' => b"\x1b[K".to_vec(),
                    b'f' => {
                        cur.fg = None;
                        b"\x1b[39m".to_vec()
                    }
                    b'k' => {
                        cur.bg = None;
                        b"\x1b[49m".to_vec()
                    }
                    _ => {
                        let spec = brace.unwrap_or_else(|| arg.unwrap_or(0).to_string().into_bytes());
                        let col = Color::parse(&spec).unwrap_or(Color::Default);
                        let col = (col != Color::Default).then_some(col);
                        match c {
                            b'K' => cur.bg = col,
                            _ => cur.fg = col,
                        }
                        color(&spec, c == b'K')
                    }
                };
                parts.push(Part::Escape(seq));
                return;
            }
            STYLE => {
                let Some(name) = brace else {
                    self.sh.error("missing style name in prompt sequence %[style]");
                    return;
                };
                let name = String::from_utf8_lossy(&name).into_owned();
                if let Err(e) = style::check_name(&name) {
                    self.sh.error(format!("prompt sequence %[style:{name}]: {e}"));
                    return;
                }
                let Some(named) = self.resolver().map(|r| r.get(&name)) else {
                    return;
                };
                let mut new = self.sgr.cur.add(&named);
                new.plain = false;
                let old = std::mem::replace(&mut self.sgr.cur, new);
                self.sgr.stack.push(old);
                parts.push(Part::Escape(sgr(&self.sgr.cur)));
                return;
            }
            STYLE_OFF => {
                if self.resolver().is_none() {
                    return;
                }
                self.sgr.cur = self.sgr.stack.pop().unwrap_or_default();
                parts.push(Part::Escape(sgr(&self.sgr.cur)));
                return;
            }
            b'{' => {
                self.in_escape = true;
                return;
            }
            b'}' => {
                self.in_escape = false;
                return;
            }
            b'G' => {
                parts.push(Part::Glitch(arg.unwrap_or(1).clamp(0, MAX_GLITCH as i64) as usize));
                return;
            }
            // Unknown sequences expand to nothing, as in zsh.
            _ => return,
        };
        self.push(parts, text);
    }

    /// The styles of the scheme in use, or None with `$NO_COLOR`, when
    /// named styles do nothing.
    fn resolver(&mut self) -> Option<&style::Resolver<'a>> {
        let sh = self.sh;
        self.styles
            .get_or_insert_with(|| {
                let no_color = sh.get_var(b"NO_COLOR").is_some_and(|v| !v.is_empty());
                let scheme = crate::builtins::style::scheme_in_use(sh);
                (!no_color).then(|| sh.styles.resolver(scheme.as_deref()))
            })
            .as_ref()
    }

    /// The condition `c` of `%(c.yes.no)`, with its number `n`.
    fn test(&mut self, c: u8, n: i64) -> bool {
        let sh = self.sh;
        match c {
            b'!' => sys::geteuid() == 0,
            b'#' => i64::from(sys::geteuid()) == n,
            b'g' => i64::from(sys::getegid()) == n,
            b'?' => i64::from(sh.last_status) == n,
            b'/' | b'C' => depth(&dir(sh, 0, false)) >= n,
            b'~' | b'.' | b'c' => depth(&dir(sh, 0, true)) >= n,
            b'j' => sh.jobs.order().len() as i64 >= n,
            b'L' => shlvl(sh) >= n,
            b'T' => i64::from(self.time().tm_hour) == n,
            b't' => i64::from(self.time().tm_min) == n,
            b'd' => i64::from(self.time().tm_mday) == n,
            b'D' => i64::from(self.time().tm_mon) == n,
            b'w' => i64::from(self.time().tm_wday) == n,
            _ => false,
        }
    }
}

/// The long names of the sequences, `%[name]`, and their letters.
const SEQUENCES: &[(&str, u8)] = &[
    ("dir", b'~'),
    ("pwd", b'/'),
    ("dir_tail", b'c'),
    ("pwd_tail", b'C'),
    ("user", b'n'),
    ("host", b'm'),
    ("hostname", b'M'),
    ("prompt_char", b'#'),
    ("status", b'?'),
    ("history", b'h'),
    ("jobs", b'j'),
    ("shlvl", b'L'),
    ("lineno", b'i'),
    ("tty", b'y'),
    ("tty_short", b'l'),
    ("date", b'D'),
    ("date_weekday", b'w'),
    ("date_us", b'W'),
    ("time", b'T'),
    ("time_seconds", b'*'),
    ("time_12h", b't'),
    ("bold", b'B'),
    ("bold_off", b'b'),
    ("underline", b'U'),
    ("underline_off", b'u'),
    ("standout", b'S'),
    ("standout_off", b's'),
    ("fg", b'F'),
    ("fg_off", b'f'),
    ("bg", b'K'),
    ("bg_off", b'k'),
    ("clear_eol", b'E'),
    ("style", STYLE),
    ("style_off", STYLE_OFF),
    ("percent", b'%'),
];

/// The long names of the conditions, `%([name].yes.no)`.
const CONDITIONS: &[(&str, u8)] = &[
    ("status", b'?'),
    ("root", b'!'),
    ("uid", b'#'),
    ("gid", b'g'),
    ("jobs", b'j'),
    ("shlvl", b'L'),
    ("pwd", b'/'),
    ("dir", b'~'),
    ("hour", b'T'),
    ("minute", b't'),
    ("day", b'd'),
    ("month", b'D'),
    ("weekday", b'w'),
];

/// Whether `%c` takes a `{...}` argument.
fn takes_brace(c: u8) -> bool {
    matches!(c, b'D' | b'F' | b'K' | STYLE)
}

/// A long name as it is compared: case, `_` and `-` don't matter.
fn name_key(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|&&c| c != b'_' && c != b'-')
        .map(u8::to_ascii_lowercase)
        .collect()
}

/// The name in `table` closest to `key` (as `name_key` gives it), if one
/// is close enough to be a likely typo: one that starts with it, or within
/// an edit distance of 2 (1 for short names).
fn suggest<'a>(table: &[(&'a str, u8)], key: &[u8]) -> Option<&'a str> {
    if key.is_empty() {
        return None;
    }
    let max = if key.len() <= 3 { 1 } else { 2 };
    table
        .iter()
        .map(|&(n, _)| {
            let k = name_key(n.as_bytes());
            let d = if k.starts_with(key) { 0 } else { distance(key, &k) };
            (d, n)
        })
        .filter(|&(d, _)| d <= max)
        .min_by_key(|&(d, _)| d)
        .map(|(_, n)| n)
}

/// The Levenshtein distance between two strings.
pub(crate) fn distance(a: &[u8], b: &[u8]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(diag + usize::from(ca != cb));
            diag = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// Applies a truncation to the parts from its start: if their text is
/// longer than allowed, text is removed on one side and replaced by the
/// marker. Escape sequences are kept.
fn truncate(parts: &mut Vec<Part>, t: Trunc) {
    let chars = |b: &[u8]| b.iter().filter(|&&c| c & 0xc0 != 0x80).count();
    let total: usize = parts[t.start..]
        .iter()
        .map(|p| match p {
            Part::Text(s) => chars(s),
            Part::Glitch(n) => *n,
            Part::Escape(_) => 0,
        })
        .sum();
    if total <= t.len {
        return;
    }
    let keep = t.len.saturating_sub(chars(&t.marker));
    let mut drop = total - keep;
    let range = t.start..parts.len();
    let mut idx: Vec<usize> = range.collect();
    if !t.left {
        idx.reverse();
    }
    for i in idx {
        if drop == 0 {
            break;
        }
        if let Part::Text(s) = &mut parts[i] {
            let n = chars(s);
            if n <= drop {
                drop -= n;
                s.clear();
                continue;
            }
            // Remove `drop` characters from the start or the end.
            if t.left {
                let mut seen = 0;
                let cut = s
                    .iter()
                    .position(|&c| {
                        if c & 0xc0 != 0x80 {
                            seen += 1;
                        }
                        seen > drop
                    })
                    .unwrap_or(s.len());
                s.drain(..cut);
            } else {
                let mut seen = 0;
                let cut = s
                    .iter()
                    .rposition(|&c| {
                        if c & 0xc0 != 0x80 {
                            seen += 1;
                        }
                        seen == drop
                    })
                    .unwrap_or(0);
                s.truncate(cut);
            }
            drop = 0;
        }
    }
    parts.retain(|p| !matches!(p, Part::Text(s) if s.is_empty()));
    let at = if t.left { t.start.min(parts.len()) } else { parts.len() };
    if !t.marker.is_empty() {
        parts.insert(at, Part::Text(t.marker));
    }
}

/// The current directory, with `~` for `$HOME` if `tilde`, and only its
/// last `n` components if `n` is positive, or its first `-n` if negative.
fn dir(sh: &Shell, n: i64, tilde: bool) -> Vec<u8> {
    let mut path = sh.curdir.clone().or_else(|| sh.get_var(b"PWD")).unwrap_or_default();
    if tilde && let Some(home) = sh.get_var(b"HOME") {
        let home = home.strip_suffix(b"/").unwrap_or(&home);
        if !home.is_empty()
            && let Some(rest) = path.strip_prefix(home)
            && (rest.is_empty() || rest[0] == b'/')
        {
            path = [b"~", rest].concat();
        }
    }
    if n == 0 {
        return path;
    }
    let comps: Vec<&[u8]> = path.split(|&c| c == b'/').filter(|c| !c.is_empty()).collect();
    let k = n.unsigned_abs() as usize;
    if k >= comps.len() {
        return path;
    }
    if n > 0 {
        comps[comps.len() - k..].join(&b'/')
    } else {
        let lead = comps[..k].join(&b'/');
        if path.first() == Some(&b'/') {
            [b"/", lead.as_slice()].concat()
        } else {
            lead
        }
    }
}

/// The number of components of a directory as `dir` gives it (`/` has
/// none, `~` counts as one).
fn depth(path: &[u8]) -> i64 {
    path.split(|&c| c == b'/').filter(|c| !c.is_empty()).count() as i64
}

/// The first `n` components of a host name, or the last `-n` (all for 0).
fn host_components(host: &[u8], n: i64) -> Vec<u8> {
    let comps: Vec<&[u8]> = host.split(|&c| c == b'.').collect();
    let k = n.unsigned_abs() as usize;
    if n == 0 || k >= comps.len() {
        host.to_vec()
    } else if n > 0 {
        comps[..k].join(&b'.')
    } else {
        comps[comps.len() - k..].join(&b'.')
    }
}

/// The user's name, looked up once.
fn user_name() -> Vec<u8> {
    static NAME: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| sys::user_name().unwrap_or_default()).clone()
}

fn shlvl(sh: &Shell) -> i64 {
    sh.get_var(b"SHLVL")
        .and_then(|v| std::str::from_utf8(v.trim_ascii()).ok()?.parse().ok())
        .unwrap_or(0)
}

/// The event number of the next history entry, or 0 without history.
fn history_number() -> usize {
    crate::interactive::with_history(|h| h.next_event()).unwrap_or(0)
}

/// The SGR sequence that sets the attributes and colours of `s`, from the
/// terminal's defaults.
fn sgr(s: &Style) -> Vec<u8> {
    let params = s.sgr();
    let sep = if params.is_empty() { "" } else { ";" };
    format!("\x1b[0{sep}{params}m").into_bytes()
}

/// The SGR sequence for `%F{spec}` (or `%K{spec}` for the background): a
/// colour as styles take it (`crate::style::Color`). Anything else is the
/// default colour.
fn color(spec: &[u8], bg: bool) -> Vec<u8> {
    let mut code = String::from("\x1b[");
    Color::parse(spec).unwrap_or(Color::Default).sgr(bg, &mut code);
    code.push('m');
    code.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(v: Vec<Part>) -> Vec<String> {
        v.into_iter()
            .map(|p| match p {
                Part::Text(s) => String::from_utf8(s).unwrap(),
                Part::Escape(s) => format!("<{}>", String::from_utf8(s).unwrap().escape_debug()),
                Part::Glitch(n) => format!("<G{n}>"),
            })
            .collect()
    }

    fn text(s: &str) -> Part {
        Part::Text(s.as_bytes().to_vec())
    }

    #[test]
    fn truncation() {
        let t = |len, marker: &str, left, v| {
            let mut v = v;
            truncate(
                &mut v,
                Trunc {
                    start: 0,
                    len,
                    marker: marker.as_bytes().to_vec(),
                    left,
                },
            );
            parts(v).concat()
        };
        assert_eq!(t(5, "..", true, vec![text("~/a/b/c")]), "..b/c");
        assert_eq!(t(5, "..", false, vec![text("~/a/b/c")]), "~/a..");
        assert_eq!(t(9, "..", true, vec![text("~/a/b/c")]), "~/a/b/c");
        assert_eq!(
            t(4, "", true, vec![text("ab"), Part::Escape(b"x".to_vec()), text("cdé")]),
            "b<x>cdé"
        );
        assert_eq!(t(3, "", false, vec![text("aé"), text("cd")]), "aéc");
        // A marker longer than the limit replaces the text.
        assert_eq!(t(2, "...", true, vec![text("abcd")]), "...");
    }

    #[test]
    fn glitch_limit() {
        // More columns than a terminal has would only fill memory.
        let p = expand(&Shell::new(), b"%{x%2222222222G%}%{y%3G%}");
        assert_eq!(p.plain.map(|p| p.len()), Some(MAX_GLITCH + 3));
    }

    #[test]
    fn colors() {
        assert_eq!(color(b"red", false), b"\x1b[31m");
        assert_eq!(color(b"9", false), b"\x1b[91m");
        assert_eq!(color(b"123", true), b"\x1b[48;5;123m");
        assert_eq!(color(b"#ff0080", false), b"\x1b[38;2;255;0;128m");
        assert_eq!(color(b"#f08", false), b"\x1b[38;2;255;0;136m");
        assert_eq!(color(b"bogus", false), b"\x1b[39m");
        assert_eq!(color(b"256", true), b"\x1b[49m");
        assert_eq!(color(b"bright-red", false), b"\x1b[91m");
    }

    #[test]
    fn long_names() {
        // The names are distinct as they are compared.
        for table in [SEQUENCES, CONDITIONS] {
            let mut keys: Vec<Vec<u8>> = table.iter().map(|(n, _)| name_key(n.as_bytes())).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), table.len());
        }
        assert_eq!(suggest(SEQUENCES, &name_key(b"hostnme")), Some("hostname"));
        assert_eq!(suggest(SEQUENCES, &name_key(b"Host-Nam")), Some("hostname"));
        assert_eq!(suggest(SEQUENCES, &name_key(b"usr")), Some("user"));
        assert_eq!(suggest(SEQUENCES, &name_key(b"und")), Some("underline"));
        assert_eq!(suggest(SEQUENCES, &name_key(b"branch")), None);
        assert_eq!(suggest(CONDITIONS, &name_key(b"stauts")), Some("status"));
        assert_eq!(distance(b"kitten", b"sitting"), 3);
        assert_eq!(distance(b"", b"ab"), 2);
    }

    #[test]
    fn hosts() {
        assert_eq!(host_components(b"a.b.c", 1), b"a");
        assert_eq!(host_components(b"a.b.c", 2), b"a.b");
        assert_eq!(host_components(b"a.b.c", -1), b"c");
        assert_eq!(host_components(b"a.b.c", 0), b"a.b.c");
        assert_eq!(host_components(b"a", 3), b"a");
    }
}
