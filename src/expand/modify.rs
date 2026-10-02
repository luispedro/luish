//! zsh's modifiers of file names (`:h`, `:t`, `:r`, `:e`, `:a`, `:A`, `:u`,
//! `:l`), for parameter expansion (`${x:h}`), glob qualifiers (`*(:t)`) and
//! the `:a` and `:A` of history expansion, with zsh's results.

use crate::builtins::cd::canonicalize;
use crate::sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// `:h`: the directory part. `:hN` keeps the first `N` components
    /// instead (a leading `/` is one).
    Head(u32),
    /// `:t`: the last component. `:tN` keeps the last `N`.
    Tail(u32),
    /// `:r`: without the extension.
    Root,
    /// `:e`: the extension.
    Ext,
    /// `:a`: absolute, with `.` and `..` removed (without looking at the
    /// file system for them).
    Absolute,
    /// `:A`: as `:a`, then with symbolic links resolved (as far as the path
    /// exists).
    Resolve,
    /// `:u`
    Upper,
    /// `:l`
    Lower,
}

impl Modifier {
    /// Reads a modifier's letter, and the count of `:h` or `:t`, at `s[*i]`.
    pub fn read(s: &[u8], i: &mut usize) -> Option<Modifier> {
        let m = match s.get(*i)? {
            b'h' => Modifier::Head(0),
            b't' => Modifier::Tail(0),
            b'r' => Modifier::Root,
            b'e' => Modifier::Ext,
            b'a' => Modifier::Absolute,
            b'A' => Modifier::Resolve,
            b'u' => Modifier::Upper,
            b'l' => Modifier::Lower,
            _ => return None,
        };
        *i += 1;
        if let Modifier::Head(_) | Modifier::Tail(_) = m {
            let digits = s[*i..].iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 {
                // A count beyond the components is all of them.
                let n = std::str::from_utf8(&s[*i..*i + digits])
                    .unwrap()
                    .parse()
                    .unwrap_or(u32::MAX);
                *i += digits;
                return Some(match m {
                    Modifier::Head(_) => Modifier::Head(n),
                    _ => Modifier::Tail(n),
                });
            }
        }
        Some(m)
    }

    /// The text of the modifier, without the `:`.
    pub fn text(self) -> String {
        match self {
            Modifier::Head(0) => "h".into(),
            Modifier::Head(n) => format!("h{n}"),
            Modifier::Tail(0) => "t".into(),
            Modifier::Tail(n) => format!("t{n}"),
            Modifier::Root => "r".into(),
            Modifier::Ext => "e".into(),
            Modifier::Absolute => "a".into(),
            Modifier::Resolve => "A".into(),
            Modifier::Upper => "u".into(),
            Modifier::Lower => "l".into(),
        }
    }
}

/// Applies a modifier to a file name.
pub fn apply(p: &[u8], m: Modifier) -> Vec<u8> {
    match m {
        Modifier::Head(n) => head(p, n).to_vec(),
        Modifier::Tail(n) => tail(p, n).to_vec(),
        Modifier::Root => match extension_dot(p) {
            Some(d) => p[..d].to_vec(),
            None => p.to_vec(),
        },
        Modifier::Ext => extension_dot(p).map_or_else(Vec::new, |d| p[d + 1..].to_vec()),
        Modifier::Absolute => absolute(p),
        Modifier::Resolve => resolve(p),
        Modifier::Upper => p.to_ascii_uppercase(),
        Modifier::Lower => p.to_ascii_lowercase(),
    }
}

/// The last `.` of the last component.
fn extension_dot(p: &[u8]) -> Option<usize> {
    let slash = p.iter().rposition(|&c| c == b'/');
    p.iter()
        .rposition(|&c| c == b'.')
        .filter(|&d| slash.is_none_or(|s| d > s))
}

/// Trailing slashes are ignored. Without a count, the result is `.` (or
/// `/`) if nothing is left; a leading `//` (but not `///`) is kept, as it
/// has a meaning of its own on some systems.
fn head(p: &[u8], n: u32) -> &[u8] {
    let mut end = p.len();
    while end > 0 && p[end - 1] == b'/' {
        end -= 1;
    }
    if n > 0 && end == 0 && !p.is_empty() {
        return b"/";
    }
    if n > 0 {
        // The first `n` components, counting runs of slashes as one.
        let mut n = n;
        let mut i = 0;
        while i < end {
            if p[i] == b'/' {
                n -= 1;
                if n == 0 {
                    return &p[..i.max(1)];
                }
                while p[i + 1] == b'/' {
                    i += 1;
                }
            }
            i += 1;
        }
        return p;
    }
    let root: &[u8] = if p.first() == Some(&b'/') { b"/" } else { b"." };
    if end == 0 {
        return root;
    }
    while end > 0 && p[end - 1] != b'/' {
        end -= 1;
    }
    if end == 0 {
        return root;
    }
    let mut s = end - 1;
    while s > 0 && p[s - 1] == b'/' {
        s -= 1;
    }
    if s == 0 {
        s = if p.get(1) == Some(&b'/') && p.get(2) != Some(&b'/') {
            2
        } else {
            1
        };
    }
    &p[..s]
}

/// Trailing slashes are ignored. The last `n` components (one for 0),
/// counting runs of slashes as one, or the whole name if it has no more.
fn tail(p: &[u8], n: u32) -> &[u8] {
    let mut end = p.len();
    while end > 0 && p[end - 1] == b'/' {
        end -= 1;
    }
    let mut n = n;
    let mut i = end;
    while i > 0 {
        i -= 1;
        if p[i] == b'/' {
            n = n.saturating_sub(1);
            if n == 0 {
                return &p[i + 1..end];
            }
            while i > 0 && p[i - 1] == b'/' {
                i -= 1;
            }
        }
    }
    &p[..end]
}

/// `:a`: relative to the current directory as the system has it (as in
/// zsh, not `PWD`). An empty name stays empty.
fn absolute(p: &[u8]) -> Vec<u8> {
    if p.is_empty() {
        return Vec::new();
    }
    if p[0] == b'/' {
        return canonicalize(p);
    }
    match sys::getcwd() {
        Some(cwd) => canonicalize(&[&cwd[..], b"/", p].concat()),
        None => p.to_vec(),
    }
}

/// `:A`: `:a`, then the longest part of the path that exists with its
/// symbolic links resolved, and the rest as it is.
fn resolve(p: &[u8]) -> Vec<u8> {
    let p = absolute(p);
    if p.first() != Some(&b'/') {
        return p;
    }
    let mut end = p.len();
    loop {
        let prefix = if end == 0 { &b"/"[..] } else { &p[..end] };
        if let Some(mut r) = sys::realpath(prefix) {
            if end < p.len() {
                if r != b"/" {
                    r.push(b'/');
                }
                r.extend_from_slice(&p[end + 1..]);
            }
            return r;
        }
        if end == 0 {
            return p;
        }
        end = p[..end].iter().rposition(|&c| c == b'/').unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, text: &str) -> String {
        let mut i = 0;
        let m = Modifier::read(text.as_bytes(), &mut i).unwrap();
        assert_eq!(i, text.len());
        // The text reads back as the same modifier (`t0` is `t`).
        assert_eq!(Modifier::read(m.text().as_bytes(), &mut 0), Some(m));
        String::from_utf8(apply(p.as_bytes(), m)).unwrap()
    }

    #[test]
    fn head_and_tail() {
        for (p, h, t) in [
            ("", ".", ""),
            ("/", "/", ""),
            ("//", "/", ""),
            ("/a/", "/", "a"),
            ("a/", ".", "a"),
            ("a//b", "a", "b"),
            ("a///b///", "a", "b"),
            ("//a", "//", "a"),
            ("///a", "/", "a"),
            ("a//b//c", "a//b", "c"),
            ("/a/b", "/a", "b"),
            ("../a", "..", "a"),
            ("a", ".", "a"),
        ] {
            assert_eq!(m(p, "h"), h, "{p}:h");
            assert_eq!(m(p, "t"), t, "{p}:t");
        }
        assert_eq!(m("/a/b/c/d", "h1"), "/");
        assert_eq!(m("/a/b/c/d", "h2"), "/a");
        assert_eq!(m("/a/b/c/d", "h9"), "/a/b/c/d");
        assert_eq!(m("a/b/c", "h2"), "a/b");
        assert_eq!(m("/a/b/c/d", "t2"), "c/d");
        assert_eq!(m("/a/b/c/d", "t9"), "/a/b/c/d");
        assert_eq!(m("/a/b/c/d", "t0"), "d");
        assert_eq!(m("//", "h1"), "/");
        assert_eq!(m("", "h1"), "");
        assert_eq!(m("a//b//c", "t2"), "b//c");
        assert_eq!(m("//a", "t2"), "//a");
    }

    #[test]
    fn extensions() {
        for (p, r, e) in [
            ("a/b.c/d.tar.gz", "a/b.c/d.tar", "gz"),
            ("a.b/c", "a.b/c", ""),
            (".bashrc", "", "bashrc"),
            ("foo.", "foo", ""),
            ("a/..", "a/.", ""),
        ] {
            assert_eq!(m(p, "r"), r, "{p}:r");
            assert_eq!(m(p, "e"), e, "{p}:e");
        }
    }

    #[test]
    fn absolute_paths() {
        assert_eq!(m("", "a"), "");
        assert_eq!(m("/a/./b/../c/", "a"), "/a/c");
        assert_eq!(m("//", "a"), "/");
        assert_eq!(m("/../a", "a"), "/a");
        assert_eq!(m("/", "A"), "/");
        assert_eq!(m("/nonexistent/x/../y", "A"), "/nonexistent/y");
        assert_eq!(m("/proc/self/../nonexistent", "A"), "/proc/nonexistent");
    }
}
