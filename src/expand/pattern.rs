//! Shell pattern matching (XCU §2.13), used for globbing, `case`, and the
//! `${x#pattern}` family. Quoted characters always match literally.

use super::split::XChar;
use crate::ast::Replace;

/// One element of a compiled pattern.
#[derive(Debug, Clone)]
enum Pat {
    Lit(u8),
    Any,
    Star,
    Set { negated: bool, items: Vec<SetItem> },
}

#[derive(Debug, Clone)]
enum SetItem {
    Byte(u8),
    Range(u8, u8),
    Class(fn(u8) -> bool),
}

fn class(name: &[u8]) -> Option<fn(u8) -> bool> {
    Some(match name {
        b"alnum" => |c: u8| c.is_ascii_alphanumeric(),
        b"alpha" => |c: u8| c.is_ascii_alphabetic(),
        b"blank" => |c: u8| c == b' ' || c == b'\t',
        b"cntrl" => |c: u8| c.is_ascii_control(),
        b"digit" => |c: u8| c.is_ascii_digit(),
        b"graph" => |c: u8| c.is_ascii_graphic(),
        b"lower" => |c: u8| c.is_ascii_lowercase(),
        b"print" => |c: u8| c.is_ascii_graphic() || c == b' ',
        b"punct" => |c: u8| c.is_ascii_punctuation(),
        b"space" => |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c),
        b"upper" => |c: u8| c.is_ascii_uppercase(),
        b"xdigit" => |c: u8| c.is_ascii_hexdigit(),
        _ => return None,
    })
}

/// Parses a bracket expression starting after the `[` at `p[i]`. Returns
/// the set and the index after the closing `]`, or `None` if there is no
/// closing bracket (then `[` is literal).
fn parse_set(p: &[XChar], mut i: usize) -> Option<(Pat, usize)> {
    let mut negated = false;
    if i < p.len() && !p[i].quoted && p[i].b == b'!' {
        negated = true;
        i += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    loop {
        if i >= p.len() {
            return None;
        }
        let c = p[i];
        if c.b == b']' && !c.quoted && !first {
            return Some((Pat::Set { negated, items }, i + 1));
        }
        first = false;
        if c.b == b'[' && !c.quoted && i + 1 < p.len() && p[i + 1].b == b':' && !p[i + 1].quoted {
            // [:class:]
            let rest = &p[i + 2..];
            if let Some(end) = rest.windows(2).position(|w| w[0].b == b':' && w[1].b == b']') {
                let name: Vec<u8> = rest[..end].iter().map(|c| c.b).collect();
                if let Some(f) = class(&name) {
                    items.push(SetItem::Class(f));
                } else {
                    items.push(SetItem::Class(|_| false));
                }
                i += 2 + end + 2;
                continue;
            }
        }
        let (lo, next) = unescape(p, i);
        if next + 1 < p.len() && p[next].b == b'-' && !p[next].quoted && !(p[next + 1].b == b']' && !p[next + 1].quoted)
        {
            let (hi, after) = unescape(p, next + 1);
            items.push(SetItem::Range(lo, hi));
            i = after;
        } else {
            items.push(SetItem::Byte(lo));
            i = next;
        }
    }
}

/// An unquoted backslash (which can only come from an expansion) escapes
/// the next character.
fn unescape(p: &[XChar], i: usize) -> (u8, usize) {
    if p[i].b == b'\\' && !p[i].quoted && i + 1 < p.len() {
        (p[i + 1].b, i + 2)
    } else {
        (p[i].b, i + 1)
    }
}

fn compile(p: &[XChar]) -> Vec<Pat> {
    let mut out = Vec::with_capacity(p.len());
    let mut i = 0;
    while i < p.len() {
        let c = p[i];
        if c.quoted {
            out.push(Pat::Lit(c.b));
            i += 1;
            continue;
        }
        match c.b {
            b'*' => {
                if !matches!(out.last(), Some(Pat::Star)) {
                    out.push(Pat::Star);
                }
                i += 1;
            }
            b'?' => {
                out.push(Pat::Any);
                i += 1;
            }
            b'[' => match parse_set(p, i + 1) {
                Some((set, next)) => {
                    out.push(set);
                    i = next;
                }
                None => {
                    out.push(Pat::Lit(b'['));
                    i += 1;
                }
            },
            b'\\' if i + 1 < p.len() => {
                out.push(Pat::Lit(p[i + 1].b));
                i += 2;
            }
            b => {
                out.push(Pat::Lit(b));
                i += 1;
            }
        }
    }
    out
}

fn set_matches(negated: bool, items: &[SetItem], c: u8) -> bool {
    let hit = items.iter().any(|it| match *it {
        SetItem::Byte(b) => b == c,
        SetItem::Range(lo, hi) => lo <= c && c <= hi,
        SetItem::Class(f) => f(c),
    });
    hit != negated
}

/// Whether the element `p` could match the byte `c`: `*` could match
/// anything.
fn elem_could_match(p: Option<&Pat>, c: u8) -> bool {
    match p {
        Some(Pat::Lit(b)) => *b == c,
        Some(Pat::Set { negated, items }) => set_matches(*negated, items, c),
        _ => true,
    }
}

fn match_compiled(p: &[Pat], s: &[u8]) -> bool {
    // Classic greedy matching with backtracking to the last `*`.
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        let ok = match p.get(pi) {
            Some(Pat::Lit(b)) => *b == s[si],
            Some(Pat::Any) => true,
            Some(Pat::Set { negated, items }) => set_matches(*negated, items, s[si]),
            Some(Pat::Star) => {
                star = Some((pi, si));
                pi += 1;
                continue;
            }
            None => false,
        };
        if ok {
            pi += 1;
            si += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|x| matches!(x, Pat::Star))
}

pub struct Pattern(Vec<Pat>);

impl Pattern {
    pub fn new(p: &[XChar]) -> Pattern {
        Pattern(compile(p))
    }

    /// The lengths of the strings the pattern can match: every element but
    /// `*` matches one byte.
    fn lengths(&self, max: usize) -> std::ops::RangeInclusive<usize> {
        let min = self.0.iter().filter(|x| !matches!(x, Pat::Star)).count();
        let has_star = self.0.len() > min;
        min..=if has_star { max } else { min.min(max) }
    }

    pub fn matches(&self, s: &[u8]) -> bool {
        match_compiled(&self.0, s)
    }

    /// Whether a leading `.` in a filename is matched explicitly.
    pub fn starts_with_dot(&self) -> bool {
        matches!(self.0.first(), Some(Pat::Lit(b'.')))
    }
}

/// Whether a pattern contains an unquoted `*`, `?`, or a complete bracket
/// expression (a lone `[` matches itself).
pub fn has_meta(p: &[XChar]) -> bool {
    p.iter()
        .enumerate()
        .any(|(i, c)| !c.quoted && (matches!(c.b, b'*' | b'?') || (c.b == b'[' && parse_set(p, i + 1).is_some())))
}

#[derive(Clone, Copy)]
pub enum Trim {
    SmallestPrefix,
    LargestPrefix,
    SmallestSuffix,
    LargestSuffix,
}

/// `${x#pat}` and friends: the part of `s` that is kept. Only the prefixes
/// or suffixes whose length the pattern can match are tried, and only
/// those whose last (for a prefix) or first (for a suffix) byte the
/// pattern's last or first element can match, so that `${x#*/}` doesn't
/// match `*/` against every prefix.
pub fn trim<'s>(s: &'s [u8], pat: &[XChar], how: Trim) -> &'s [u8] {
    let p = Pattern::new(pat);
    let n = s.len();
    let mut lens = p.lengths(n);
    let (first, last) = (p.0.first(), p.0.last());
    let prefix = |k: usize| (k == 0 || elem_could_match(last, s[k - 1])) && p.matches(&s[..k]);
    let suffix = |k: usize| (k == 0 || elem_could_match(first, s[n - k])) && p.matches(&s[n - k..]);
    match how {
        Trim::SmallestPrefix => lens.find(|&k| prefix(k)).map(|k| &s[k..]),
        Trim::LargestPrefix => lens.rev().find(|&k| prefix(k)).map(|k| &s[k..]),
        Trim::SmallestSuffix => lens.find(|&k| suffix(k)).map(|k| &s[..n - k]),
        Trim::LargestSuffix => lens.rev().find(|&k| suffix(k)).map(|k| &s[..n - k]),
    }
    .unwrap_or(s)
}

/// `${x/pat/rep}` and its variants: replaces the longest match that
/// starts first (or each such match in turn, or the longest at the start,
/// or the longest at the end). An empty pattern matches only at the start
/// or the end.
pub fn replace(s: &[u8], pat: &[XChar], how: Replace, rep: &[u8]) -> Vec<u8> {
    let p = Pattern::new(pat);
    let n = s.len();
    let first = p.0.first();
    // The length of the longest match at `i`.
    let longest_at = |i: usize| {
        let could = i == n || elem_could_match(first, s[i]);
        (could.then(|| p.lengths(n - i).rev().find(|&k| p.matches(&s[i..i + k])))).flatten()
    };
    let mut out = Vec::with_capacity(n + rep.len());
    match how {
        Replace::Prefix => match longest_at(0) {
            Some(k) => {
                out.extend_from_slice(rep);
                out.extend_from_slice(&s[k..]);
            }
            None => out.extend_from_slice(s),
        },
        Replace::Suffix => {
            let last = p.0.last();
            let lens = p.lengths(n);
            // The longest suffix that matches.
            let k = lens
                .rev()
                .find(|&k| (k == 0 || elem_could_match(last, s[n - 1])) && p.matches(&s[n - k..]));
            match k {
                Some(k) => {
                    out.extend_from_slice(&s[..n - k]);
                    out.extend_from_slice(rep);
                }
                None => out.extend_from_slice(s),
            }
        }
        Replace::First | Replace::All if pat.is_empty() => out.extend_from_slice(s),
        Replace::First | Replace::All if n == 0 => {
            out.extend_from_slice(if p.matches(b"") { rep } else { s });
        }
        Replace::First | Replace::All => {
            let mut i = 0;
            while i < n {
                match longest_at(i) {
                    // A match can be empty only at the end (a pattern of
                    // `*` alone matches the whole rest).
                    Some(k) if k > 0 => {
                        out.extend_from_slice(rep);
                        i += k;
                        if how == Replace::First {
                            break;
                        }
                    }
                    _ => {
                        out.push(s[i]);
                        i += 1;
                    }
                }
            }
            out.extend_from_slice(&s[i..]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::split::unquoted;
    use super::*;

    fn m(p: &str, s: &str) -> bool {
        Pattern::new(&unquoted(p.as_bytes())).matches(s.as_bytes())
    }

    #[test]
    fn basics() {
        assert!(m("*", ""));
        assert!(m("a*c", "abbbc"));
        assert!(!m("a*c", "abbbd"));
        assert!(m("?b", "ab"));
        assert!(m("[a-c]x", "bx"));
        assert!(!m("[!a-c]x", "bx"));
        assert!(m("[]]", "]"));
        assert!(m("[[:digit:]]*", "1abc"));
        assert!(m("[", "["));
        assert!(m("*.txt", "a.b.txt"));
        assert!(m("\\*", "*"));
        assert!(!m("\\*", "a"));
    }

    #[test]
    fn quoted() {
        let mut p = unquoted(b"a*");
        p[1].quoted = true;
        assert!(Pattern::new(&p).matches(b"a*"));
        assert!(!Pattern::new(&p).matches(b"ab"));
    }

    #[test]
    fn trims() {
        let p = unquoted(b"*/");
        assert_eq!(trim(b"a/b/c", &p, Trim::SmallestPrefix), b"b/c");
        assert_eq!(trim(b"a/b/c", &p, Trim::LargestPrefix), b"c");
        let p = unquoted(b".*");
        assert_eq!(trim(b"x.tar.gz", &p, Trim::SmallestSuffix), b"x.tar");
        assert_eq!(trim(b"x.tar.gz", &p, Trim::LargestSuffix), b"x");
        let p = unquoted(b"?");
        assert_eq!(trim(b"abc", &p, Trim::SmallestPrefix), b"bc");
        assert_eq!(trim(b"abc", &p, Trim::LargestSuffix), b"ab");
        assert_eq!(trim(b"", &p, Trim::LargestPrefix), b"");
        let p = unquoted(b"a?c");
        assert_eq!(trim(b"ab", &p, Trim::SmallestPrefix), b"ab");
        assert_eq!(trim(b"abcabc", &p, Trim::LargestPrefix), b"abc");
        assert_eq!(trim(b"abcabc", &p, Trim::SmallestSuffix), b"abc");
        let p = unquoted(b"*");
        assert_eq!(trim(b"abc", &p, Trim::SmallestPrefix), b"abc");
        assert_eq!(trim(b"abc", &p, Trim::LargestPrefix), b"");
        let p = unquoted(b"a*b*");
        assert_eq!(trim(b"xab", &p, Trim::LargestSuffix), b"x");
        assert_eq!(trim(b"xab", &p, Trim::LargestPrefix), b"xab");
        let p = unquoted(b"*[/:]");
        assert_eq!(trim(b"a/b:c", &p, Trim::SmallestPrefix), b"b:c");
        assert_eq!(trim(b"a/b:c", &p, Trim::LargestPrefix), b"c");
        let p = unquoted(b"[/:]*");
        assert_eq!(trim(b"a/b:c", &p, Trim::SmallestSuffix), b"a/b");
        assert_eq!(trim(b"a/b:c", &p, Trim::LargestSuffix), b"a");
    }

    #[test]
    fn replaces() {
        let r = |s: &str, p: &str, how, rep: &str| {
            String::from_utf8(replace(s.as_bytes(), &unquoted(p.as_bytes()), how, rep.as_bytes())).unwrap()
        };
        assert_eq!(r("a.b.c", ".", Replace::First, "-"), "a-b.c");
        assert_eq!(r("a.b.c", ".", Replace::All, "-"), "a-b-c");
        assert_eq!(r("a.b.c", "a", Replace::Prefix, "X"), "X.b.c");
        assert_eq!(r("a.b.c", "c", Replace::Suffix, "X"), "a.b.X");
        assert_eq!(r("abc", "", Replace::Prefix, "P"), "Pabc");
        assert_eq!(r("abc", "", Replace::Suffix, "S"), "abcS");
        assert_eq!(r("abc", "", Replace::All, "S"), "abc");
        assert_eq!(r("aaa", "a*", Replace::All, "b"), "b");
        assert_eq!(r("aaa", "*", Replace::All, "b"), "b");
        assert_eq!(r("aaa", "a?", Replace::First, "b"), "ba");
        assert_eq!(r("abcabc", "b*", Replace::Suffix, "Q"), "aQ");
        assert_eq!(r("abcabc", "*b", Replace::Prefix, "Q"), "Qc");
        assert_eq!(r("", "*", Replace::First, "Q"), "Q");
        assert_eq!(r("", "a", Replace::All, "Q"), "");
        assert_eq!(r("abc", "[ac]", Replace::All, "_"), "_b_");
    }
}
