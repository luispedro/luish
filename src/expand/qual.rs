//! zsh-style glob qualifiers: the `(...)` at the end of a word under
//! `setopt glob.bare_qualifiers` (as in `*(/)` or `**/*.md(.om[1,3])`). They select
//! matches by file type, permissions, owner, size and times, and sort,
//! slice, mark and modify the result.

use std::cmp::Ordering;

use crate::sys;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmp {
    Less,
    Equal,
    Greater,
}

impl Cmp {
    // `v` is generic because `st_nlink` is u64 on x86_64 but u32 on aarch64.
    fn holds(self, v: impl Into<u64>, n: u64) -> bool {
        let v = v.into();
        match self {
            Cmp::Less => v < n,
            Cmp::Equal => v == n,
            Cmp::Greater => v > n,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Time {
    Access,
    Modify,
    Change,
}

#[derive(Debug, Clone, PartialEq)]
enum TestKind {
    /// The `S_IFMT` bits are this type.
    Type(u32),
    /// A block or character device.
    Device,
    /// A regular file with an execute bit.
    Executable,
    /// Any of these permission bits is set.
    Perm(u32),
    Uid(u32),
    Gid(u32),
    Dev(u64),
    Links(Cmp, u64),
    /// Size, rounded up to a multiple of the unit.
    Size(u64, Cmp, u64),
    /// Seconds since the time, divided by the unit.
    Age(Time, u64, Cmp, u64),
}

#[derive(Debug, Clone, PartialEq)]
struct Test {
    kind: TestKind,
    negated: bool,
    /// Test the target of a symbolic link (`-`).
    follow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Key {
    Name,
    Size,
    Links,
    Time(Time),
    Depth,
    Unsorted,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum Mark {
    #[default]
    None,
    /// `M`: a `/` after directories.
    Dirs,
    /// `T`: a character after each name giving its type, as `ls -F`.
    Types,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Modifier {
    Head,
    Tail,
    Root,
    Ext,
    Upper,
    Lower,
}

/// A parsed qualifier list.
#[derive(Debug, Default, PartialEq)]
pub struct Qualifiers {
    /// Alternatives separated by `,`: a file is selected if it passes every
    /// test of one of them. Empty: every file is selected.
    alts: Vec<Vec<Test>>,
    /// `N`: no matches give no words, rather than the word itself.
    pub null: bool,
    /// `D`: match hidden files.
    pub dots: bool,
    /// `n`: names sort numerically.
    numeric: bool,
    /// `o`/`O`: sort keys, with whether each is descending and follows
    /// symbolic links.
    sort: Vec<(Key, bool, bool)>,
    /// `[beg,end]`: 1-based, negative counts from the end.
    range: Option<(i64, i64)>,
    mark: Mark,
    /// The marks are for the target of a symbolic link (`-` before them).
    mark_follow: bool,
    mods: Vec<Modifier>,
}

/// A qualifier string that can't be parsed: the message.
pub type QualError = String;

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let c = self.peek();
        self.i += c.is_some() as usize;
        c
    }

    fn number(&mut self) -> Result<u64, QualError> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .unwrap()
            .parse()
            .map_err(|_| "number expected".to_string())
    }

    fn signed(&mut self) -> Result<i64, QualError> {
        let neg = self.peek() == Some(b'-');
        if neg {
            self.i += 1;
        }
        let n = self.number()? as i64;
        Ok(if neg { -n } else { n })
    }

    /// An optional `-` or `+` then a number: `-n` means less than `n`, `+n`
    /// more than `n`.
    fn comparison(&mut self) -> Result<(Cmp, u64), QualError> {
        let cmp = match self.peek() {
            Some(b'-') => Cmp::Less,
            Some(b'+') => Cmp::Greater,
            _ => Cmp::Equal,
        };
        if cmp != Cmp::Equal {
            self.i += 1;
        }
        Ok((cmp, self.number()?))
    }

    /// A user or group: a number, or a name between delimiters (`u:root:`;
    /// the brackets `[]`, `{}` and `<>` pair up).
    fn id(&mut self, group: bool) -> Result<u32, QualError> {
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Ok(self.number()? as u32);
        }
        let open = self.next().ok_or("missing delimiter")?;
        let close = match open {
            b'[' => b']',
            b'{' => b'}',
            b'<' => b'>',
            c => c,
        };
        let len = self.s[self.i..]
            .iter()
            .position(|&c| c == close)
            .ok_or("missing delimiter")?;
        let name = &self.s[self.i..self.i + len];
        self.i += len + 1;
        let id = if group { sys::group_id(name) } else { sys::user_id(name) };
        id.ok_or_else(|| {
            format!(
                "unknown {}: {}",
                if group { "group" } else { "user" },
                String::from_utf8_lossy(name)
            )
        })
    }
}

impl Qualifiers {
    pub fn parse(s: &[u8]) -> Result<Qualifiers, QualError> {
        let mut q = Qualifiers::default();
        let mut r = Reader { s, i: 0 };
        let mut alt = Vec::new();
        let (mut negated, mut follow) = (false, false);
        while let Some(c) = r.next() {
            let kind = match c {
                b'/' => TestKind::Type(libc::S_IFDIR),
                b'.' => TestKind::Type(libc::S_IFREG),
                b'@' => TestKind::Type(libc::S_IFLNK),
                b'=' => TestKind::Type(libc::S_IFSOCK),
                b'p' => TestKind::Type(libc::S_IFIFO),
                b'%' => match r.peek() {
                    Some(b'b') => {
                        r.i += 1;
                        TestKind::Type(libc::S_IFBLK)
                    }
                    Some(b'c') => {
                        r.i += 1;
                        TestKind::Type(libc::S_IFCHR)
                    }
                    _ => TestKind::Device,
                },
                b'*' => TestKind::Executable,
                b'r' => TestKind::Perm(libc::S_IRUSR),
                b'w' => TestKind::Perm(libc::S_IWUSR),
                b'x' => TestKind::Perm(libc::S_IXUSR),
                b'A' => TestKind::Perm(libc::S_IRGRP),
                b'I' => TestKind::Perm(libc::S_IWGRP),
                b'E' => TestKind::Perm(libc::S_IXGRP),
                b'R' => TestKind::Perm(libc::S_IROTH),
                b'W' => TestKind::Perm(libc::S_IWOTH),
                b'X' => TestKind::Perm(libc::S_IXOTH),
                b's' => TestKind::Perm(libc::S_ISUID),
                b'S' => TestKind::Perm(libc::S_ISGID),
                b't' => TestKind::Perm(libc::S_ISVTX),
                b'U' => TestKind::Uid(sys::geteuid()),
                b'G' => TestKind::Gid(sys::getegid()),
                b'u' => TestKind::Uid(r.id(false)?),
                b'g' => TestKind::Gid(r.id(true)?),
                b'd' => TestKind::Dev(r.number()?),
                b'l' => {
                    let (cmp, n) = r.comparison()?;
                    TestKind::Links(cmp, n)
                }
                b'L' => {
                    let unit = match r.peek() {
                        Some(b'p' | b'P') => 512,
                        Some(b'k' | b'K') => 1 << 10,
                        Some(b'm' | b'M') => 1 << 20,
                        Some(b'g' | b'G') => 1 << 30,
                        Some(b't' | b'T') => 1 << 40,
                        _ => 1,
                    };
                    if unit != 1 {
                        r.i += 1;
                    }
                    let (cmp, n) = r.comparison()?;
                    TestKind::Size(unit, cmp, n)
                }
                b'a' | b'm' | b'c' => {
                    let time = match c {
                        b'a' => Time::Access,
                        b'm' => Time::Modify,
                        _ => Time::Change,
                    };
                    let unit = match r.peek() {
                        Some(b's') => 1,
                        Some(b'm') => 60,
                        Some(b'h') => 3600,
                        Some(b'd') => 86400,
                        Some(b'w') => 7 * 86400,
                        Some(b'M') => 30 * 86400,
                        _ => 0,
                    };
                    if unit != 0 {
                        r.i += 1;
                    }
                    let (cmp, n) = r.comparison()?;
                    TestKind::Age(time, if unit == 0 { 86400 } else { unit }, cmp, n)
                }
                b'^' => {
                    negated = !negated;
                    continue;
                }
                b'-' => {
                    follow = !follow;
                    continue;
                }
                b',' => {
                    q.alts.push(std::mem::take(&mut alt));
                    (negated, follow) = (false, false);
                    continue;
                }
                b'N' => {
                    q.null = true;
                    continue;
                }
                b'D' => {
                    q.dots = true;
                    continue;
                }
                b'n' => {
                    q.numeric = true;
                    continue;
                }
                b'M' | b'T' => {
                    q.mark = if c == b'M' { Mark::Dirs } else { Mark::Types };
                    q.mark_follow = follow;
                    continue;
                }
                b'o' | b'O' => {
                    let key = match r.next() {
                        Some(b'n') => Key::Name,
                        Some(b'L') => Key::Size,
                        Some(b'l') => Key::Links,
                        Some(b'a') => Key::Time(Time::Access),
                        Some(b'm') => Key::Time(Time::Modify),
                        Some(b'c') => Key::Time(Time::Change),
                        Some(b'd') => Key::Depth,
                        Some(b'N') => Key::Unsorted,
                        _ => return Err("unknown sort specifier".into()),
                    };
                    q.sort.push((key, c == b'O', follow));
                    continue;
                }
                b'[' => {
                    let beg = r.signed()?;
                    let end = if r.peek() == Some(b',') {
                        r.i += 1;
                        r.signed()?
                    } else {
                        beg
                    };
                    if r.next() != Some(b']') {
                        return Err("bad subscript".into());
                    }
                    q.range = Some((beg, end));
                    continue;
                }
                b':' => {
                    r.i -= 1;
                    q.mods = parse_modifiers(&mut r)?;
                    break;
                }
                c => return Err(format!("unknown file attribute: {}", c as char)),
            };
            alt.push(Test { kind, negated, follow });
        }
        if !alt.is_empty() || !q.alts.is_empty() {
            q.alts.push(alt);
        }
        Ok(q)
    }

    /// Selects, sorts, slices, marks and modifies `paths` (the matches of
    /// the pattern, sorted by name). `None` if no file was selected; as in
    /// zsh, a subscript that leaves nothing of the selected files gives an
    /// empty list instead.
    pub fn apply(&self, paths: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let now = sys::now();
        let mut files: Vec<File> = paths
            .into_iter()
            .filter_map(|path| {
                let lst = sys::lstat(&path)?;
                let st = if lst.st_mode & libc::S_IFMT == libc::S_IFLNK {
                    sys::stat(&path).unwrap_or(lst)
                } else {
                    lst
                };
                Some(File { path, lst, st })
            })
            .filter(|f| self.alts.is_empty() || self.alts.iter().any(|alt| alt.iter().all(|t| f.passes(t, now))))
            .collect();
        if self.numeric {
            files.sort_by(|a, b| numeric_cmp(&a.path, &b.path));
        }
        if !self.sort.is_empty() {
            files.sort_by(|a, b| {
                for &(key, desc, follow) in &self.sort {
                    let o = a.compare(b, key, follow, self.numeric);
                    let o = if desc { o.reverse() } else { o };
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                Ordering::Equal
            });
        }
        if files.is_empty() {
            return None;
        }
        if let Some((beg, end)) = self.range {
            let n = files.len() as i64;
            let index = |i: i64| if i < 0 { n + i } else { i };
            let (b, e) = (index(beg).max(0), index(end).min(n - 1));
            files = if b <= e {
                files.drain(b as usize..=e as usize).collect()
            } else {
                Vec::new()
            };
        }
        Some(
            files
                .into_iter()
                .map(|f| {
                    let st = if self.mark_follow { &f.st } else { &f.lst };
                    let mark = match self.mark {
                        Mark::None => None,
                        Mark::Dirs => (st.st_mode & libc::S_IFMT == libc::S_IFDIR).then_some(b'/'),
                        Mark::Types => Some(type_char(st)),
                    };
                    let mut p = f.path;
                    for &m in &self.mods {
                        p = modify(&p, m);
                    }
                    p.extend(mark);
                    p
                })
                .collect(),
        )
    }
}

fn parse_modifiers(r: &mut Reader) -> Result<Vec<Modifier>, QualError> {
    let mut mods = Vec::new();
    while let Some(c) = r.next() {
        let m = if c == b':' { r.next() } else { Some(c) };
        mods.push(match m {
            Some(b'h') if c == b':' => Modifier::Head,
            Some(b't') if c == b':' => Modifier::Tail,
            Some(b'r') if c == b':' => Modifier::Root,
            Some(b'e') if c == b':' => Modifier::Ext,
            Some(b'u') if c == b':' => Modifier::Upper,
            Some(b'l') if c == b':' => Modifier::Lower,
            Some(c) => return Err(format!("unrecognized modifier `{}'", c as char)),
            None => return Err("unrecognized modifier".into()),
        });
    }
    Ok(mods)
}

/// The `ls -F` character for a file's type, or a space (as in zsh).
fn type_char(st: &libc::stat) -> u8 {
    match st.st_mode & libc::S_IFMT {
        libc::S_IFDIR => b'/',
        libc::S_IFLNK => b'@',
        libc::S_IFIFO => b'|',
        libc::S_IFSOCK => b'=',
        libc::S_IFBLK => b'#',
        libc::S_IFCHR => b'%',
        libc::S_IFREG if st.st_mode & 0o111 != 0 => b'*',
        _ => b' ',
    }
}

/// The `:h`, `:t`, `:r`, `:e`, `:u` and `:l` modifiers of zsh (and csh).
fn modify(p: &[u8], m: Modifier) -> Vec<u8> {
    let slash = p.iter().rposition(|&c| c == b'/');
    let dot = p
        .iter()
        .rposition(|&c| c == b'.')
        .filter(|&d| slash.is_none_or(|s| d > s));
    match m {
        Modifier::Head => match slash {
            Some(0) => b"/".to_vec(),
            Some(s) => p[..s].to_vec(),
            None => b".".to_vec(),
        },
        Modifier::Tail => p[slash.map_or(0, |s| s + 1)..].to_vec(),
        Modifier::Root => p[..dot.unwrap_or(p.len())].to_vec(),
        Modifier::Ext => dot.map_or_else(Vec::new, |d| p[d + 1..].to_vec()),
        Modifier::Upper => p.to_ascii_uppercase(),
        Modifier::Lower => p.to_ascii_lowercase(),
    }
}

/// Compares names with runs of digits compared as numbers.
fn numeric_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let run = |s: &[u8], k: usize| k + s[k..].iter().take_while(|c| c.is_ascii_digit()).count();
            let (ei, ej) = (run(a, i), run(b, j));
            let trim = |s: &[u8]| {
                let z = s.iter().take_while(|&&c| c == b'0').count();
                s[z..].to_vec()
            };
            let (x, y) = (trim(&a[i..ei]), trim(&b[j..ej]));
            let o = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
            if o != Ordering::Equal {
                return o;
            }
            (i, j) = (ei, ej);
        } else {
            if a[i] != b[j] {
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j)).then_with(|| a.cmp(b))
}

/// Orders paths so that at each level, what is in subdirectories comes
/// before the files in the directory itself (`od`). Other paths are equal.
fn depth_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let mut x = a.split(|&c| c == b'/');
    let mut y = b.split(|&c| c == b'/');
    loop {
        match (x.next(), y.next()) {
            (Some(p), Some(q)) if p == q => continue,
            (Some(_), Some(_)) => {
                let (deeper_a, deeper_b) = (x.next().is_some(), y.next().is_some());
                return deeper_b.cmp(&deeper_a);
            }
            (a, b) => return b.is_some().cmp(&a.is_some()),
        }
    }
}

struct File {
    path: Vec<u8>,
    lst: libc::stat,
    /// The target of a symbolic link (the link itself if it is broken).
    st: libc::stat,
}

fn time_of(st: &libc::stat, t: Time) -> i64 {
    match t {
        Time::Access => st.st_atime,
        Time::Modify => st.st_mtime,
        Time::Change => st.st_ctime,
    }
}

impl File {
    fn passes(&self, t: &Test, now: i64) -> bool {
        let st = if t.follow { &self.st } else { &self.lst };
        let mode = st.st_mode;
        let hit = match t.kind {
            TestKind::Type(ty) => mode & libc::S_IFMT == ty,
            TestKind::Device => matches!(mode & libc::S_IFMT, libc::S_IFBLK | libc::S_IFCHR),
            TestKind::Executable => mode & libc::S_IFMT == libc::S_IFREG && mode & 0o111 != 0,
            TestKind::Perm(bits) => mode & bits != 0,
            TestKind::Uid(u) => st.st_uid == u,
            TestKind::Gid(g) => st.st_gid == g,
            TestKind::Dev(d) => st.st_dev == d,
            TestKind::Links(cmp, n) => cmp.holds(st.st_nlink, n),
            TestKind::Size(unit, cmp, n) => cmp.holds((st.st_size as u64).div_ceil(unit), n),
            TestKind::Age(time, unit, cmp, n) => cmp.holds((now - time_of(st, time)).max(0) as u64 / unit, n),
        };
        hit != t.negated
    }

    fn compare(&self, other: &File, key: Key, follow: bool, numeric: bool) -> Ordering {
        let (a, b) = if follow {
            (&self.st, &other.st)
        } else {
            (&self.lst, &other.lst)
        };
        match key {
            Key::Name if numeric => numeric_cmp(&self.path, &other.path),
            Key::Name => self.path.cmp(&other.path),
            Key::Size => a.st_size.cmp(&b.st_size),
            Key::Links => a.st_nlink.cmp(&b.st_nlink),
            // Most recent first.
            Key::Time(t) => time_of(b, t).cmp(&time_of(a, t)),
            Key::Depth => depth_cmp(&self.path, &other.path),
            Key::Unsorted => Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Qualifiers, QualError> {
        Qualifiers::parse(s.as_bytes())
    }

    #[test]
    fn parses() {
        let q = parse("/").unwrap();
        assert_eq!(q.alts.len(), 1);
        assert_eq!(q.alts[0][0].kind, TestKind::Type(libc::S_IFDIR));
        let q = parse("^-/,.N").unwrap();
        assert!(q.null);
        assert!(q.alts[0][0].negated && q.alts[0][0].follow);
        assert!(!q.alts[1][0].negated);
        let q = parse("Lk+10om[1,-2]").unwrap();
        assert_eq!(q.alts[0][0].kind, TestKind::Size(1024, Cmp::Greater, 10));
        assert_eq!(q.sort, [(Key::Time(Time::Modify), false, false)]);
        assert_eq!(q.range, Some((1, -2)));
        let q = parse("mh-2").unwrap();
        assert_eq!(q.alts[0][0].kind, TestKind::Age(Time::Modify, 3600, Cmp::Less, 2));
        let q = parse(".:t:r").unwrap();
        assert_eq!(q.mods, [Modifier::Tail, Modifier::Root]);
        assert!(parse("DN").unwrap().alts.is_empty());
        assert!(parse("Z").unwrap_err().contains("unknown file attribute: Z"));
        assert!(parse("L").is_err());
        assert!(parse("oz").is_err());
        assert!(parse(":z").is_err());
        assert!(parse("[1").is_err());
    }

    #[test]
    fn modifiers() {
        let m = |p: &str, m| String::from_utf8(modify(p.as_bytes(), m)).unwrap();
        assert_eq!(m("a/b.c/d.tar.gz", Modifier::Head), "a/b.c");
        assert_eq!(m("d", Modifier::Head), ".");
        assert_eq!(m("/d", Modifier::Head), "/");
        assert_eq!(m("a/b.c/d.tar.gz", Modifier::Tail), "d.tar.gz");
        assert_eq!(m("a/b.c/d.tar.gz", Modifier::Root), "a/b.c/d.tar");
        assert_eq!(m("a/b.c/d", Modifier::Root), "a/b.c/d");
        assert_eq!(m("a/b.c/d.tar.gz", Modifier::Ext), "gz");
        assert_eq!(m("a/b.c/d", Modifier::Ext), "");
    }

    #[test]
    fn orders() {
        assert_eq!(numeric_cmp(b"a10", b"a9"), Ordering::Greater);
        assert_eq!(numeric_cmp(b"a010", b"a9"), Ordering::Greater);
        assert_eq!(numeric_cmp(b"a2b", b"a2c"), Ordering::Less);
        assert_eq!(depth_cmp(b"a/b/c", b"a/x"), Ordering::Less);
        assert_eq!(depth_cmp(b"a/x", b"b"), Ordering::Less);
        assert_eq!(depth_cmp(b"c", b"a/x"), Ordering::Greater);
        assert_eq!(depth_cmp(b"a", b"b"), Ordering::Equal);
        assert_eq!(depth_cmp(b"a", b"a/b"), Ordering::Greater);
    }
}
