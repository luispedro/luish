//! The history file, in zsh's format, so that luish and zsh can share it.
//!
//! Each entry is `: START:ELAPSED;COMMAND` (zsh's extended format, START in
//! seconds since the epoch), or only `COMMAND` (zsh without
//! `extended_history`). A newline in a command is written as `\` and a
//! newline, and bytes that zsh uses internally are "metafied" (0x83, then
//! the byte xor 32), as zsh writes them. luish's older format (a `#V2`
//! line, then one entry per line with `\` and newline escaped) is still
//! read.
//!
//! Shells append to the file under an `fcntl` lock on the file itself (as
//! zsh does with `hist_fcntl_lock`), and replace it through a temporary
//! file when it has grown too long. [`FileState`] remembers how much of the
//! file a shell has seen, so that `history.share` can read only what other
//! shells have added since.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, FileExt, MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;

use super::to_path;
use crate::sys;

/// zsh's `Meta` byte.
const META: u8 = 0x83;

pub struct Record {
    pub text: String,
    pub time: i64,
    pub elapsed: i64,
    /// The offset just after the record, in the data parsed.
    pub end: usize,
}

/// Whether zsh metafies a byte: NUL, and `Meta` to `Marker`, the bytes zsh
/// uses as tokens.
fn is_meta(b: u8) -> bool {
    b == 0 || (META..=0xa2).contains(&b)
}

fn metafy(text: &[u8], out: &mut Vec<u8>) {
    for &b in text {
        if b == b'\n' {
            out.extend_from_slice(b"\\\n");
        } else if is_meta(b) {
            out.extend_from_slice(&[META, b ^ 32]);
        } else {
            out.push(b);
        }
    }
}

fn unmetafy(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut it = text.iter();
    while let Some(&b) = it.next() {
        match b {
            META => out.extend(it.next().map(|c| c ^ 32)),
            _ => out.push(b),
        }
    }
    out
}

/// The text of a line of luish's old format.
fn unescape_v2(line: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len());
    let mut it = line.iter();
    while let Some(&b) = it.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match it.next() {
            Some(b'n') => out.push(b'\n'),
            Some(&c) => out.push(c),
            None => out.push(b'\\'),
        }
    }
    out
}

/// Splits `: START:ELAPSED;COMMAND` into its parts.
fn extended(line: &[u8]) -> Option<(i64, i64, &[u8])> {
    let rest = line.strip_prefix(b": ")?;
    let number = |s: &[u8]| -> Option<(i64, usize)> {
        let n = s.iter().take_while(|c| c.is_ascii_digit()).count();
        let v = std::str::from_utf8(&s[..n]).ok()?.parse().ok()?;
        Some((v, n))
    };
    let (time, n) = number(rest)?;
    let rest = rest[n..].strip_prefix(b":")?;
    let (elapsed, n) = number(rest)?;
    Some((time, elapsed, rest[n..].strip_prefix(b";")?))
}

/// Parses the entries in `data`, the whole file (which may start with the
/// `#V2` of luish's old format) if `whole`, otherwise a part of it after
/// an entry. With `complete_only`, an entry that doesn't end with a newline
/// (which another shell may be writing) is left out.
pub fn parse(data: &[u8], whole: bool, complete_only: bool) -> Vec<Record> {
    let mut records = Vec::new();
    let mut pos = 0;
    let v2 = whole && data.starts_with(b"#V2\n");
    if v2 {
        pos = 4;
    }
    while pos < data.len() {
        let line_end = |from: usize| data[from..].iter().position(|&c| c == b'\n').map(|i| from + i);
        let mut end = line_end(pos);
        let mut line = &data[pos..end.unwrap_or(data.len())];
        let (time, elapsed, text) = match extended(line) {
            Some((time, elapsed, rest)) => {
                line = rest;
                (time, elapsed, None)
            }
            None if v2 => (0, 0, Some(unescape_v2(line))),
            None => (0, 0, None),
        };
        let text = match text {
            Some(t) => t,
            None => {
                // zsh's format: a line ending in `\` continues on the next.
                let mut raw = line.to_vec();
                while let Some(e) = end
                    && raw.last() == Some(&b'\\')
                {
                    if e + 1 == data.len() {
                        // The rest of the entry hasn't been written yet.
                        end = end.filter(|_| !complete_only);
                        break;
                    }
                    raw.pop();
                    raw.push(b'\n');
                    end = line_end(e + 1);
                    raw.extend_from_slice(&data[e + 1..end.unwrap_or(data.len())]);
                }
                unmetafy(&raw)
            }
        };
        let next = match end {
            Some(e) => e + 1,
            None if complete_only => break,
            None => data.len(),
        };
        if !text.is_empty() {
            let text = String::from_utf8(text).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
            records.push(Record {
                text,
                time,
                elapsed,
                end: next,
            });
        }
        pos = next;
    }
    records
}

/// Appends an entry in zsh's extended format (or its plain one, without a
/// time).
pub fn format(text: &str, time: i64, elapsed: i64, out: &mut Vec<u8>) {
    if time > 0 {
        out.extend_from_slice(format!(": {time}:{elapsed};").as_bytes());
    }
    metafy(text.as_bytes(), out);
    // As in zsh, so that the entry doesn't continue on the next line.
    if text.ends_with('\\') {
        out.push(b' ');
    }
    out.push(b'\n');
}

/// How much of a history file a shell has seen.
#[derive(Default)]
pub struct FileState {
    pub path: Vec<u8>,
    /// The device and inode of the file.
    id: (u64, u64),
    /// The size of the file when last read or written...
    offset: u64,
    /// ...and its last entry, as written, to check that the file is the
    /// same up to `offset` (and find the entry in it if it was replaced).
    last: Vec<u8>,
    /// The number of entries in the file, as far as the shell knows: it
    /// doesn't count those other shells have added unless it reads them.
    pub count: usize,
}

impl FileState {
    /// The state of a shell that hasn't read the file at `path`.
    pub fn new(path: &[u8]) -> FileState {
        FileState {
            path: path.to_vec(),
            ..FileState::default()
        }
    }

    /// The state of a shell that has read all of `data`, the contents of
    /// `file`.
    pub fn read(path: &[u8], file: Option<&File>, data: &[u8], records: &[Record]) -> FileState {
        let mut st = FileState::new(path);
        if let Some(m) = file.and_then(|f| f.metadata().ok()) {
            st.id = (m.dev(), m.ino());
        }
        st.seen(data, 0, records);
        st.count = records.len();
        st
    }

    /// Records that the shell has seen `records`, parsed from `data`, the
    /// part of the file from `base`.
    fn seen(&mut self, data: &[u8], base: u64, records: &[Record]) {
        let end = records.last().map_or(0, |r| r.end);
        let start = records.len().checked_sub(2).map_or(0, |i| records[i].end);
        if !records.is_empty() {
            self.last = data[start..end].to_vec();
            self.offset = base + end as u64;
        }
    }

    /// Whether the file may have changed since the shell last read or
    /// wrote it.
    pub fn changed(&self) -> bool {
        sys::stat(&self.path).is_some_and(|s| (s.st_dev, s.st_ino) != self.id || s.st_size as u64 != self.offset)
    }

    /// The entries other shells have added to `file` since the shell last
    /// read or wrote it. After it was replaced, those after the entry that
    /// was last before; if that entry is gone, none.
    pub fn new_records(&mut self, file: &File) -> Vec<Record> {
        let Ok(m) = file.metadata() else { return Vec::new() };
        let id = (m.dev(), m.ino());
        let size = m.len();
        let last = self.last.len() as u64;
        if id == self.id && size == self.offset {
            return Vec::new();
        }
        if id == self.id && size > self.offset && self.offset >= last {
            let mut data = vec![0; (size - self.offset + last) as usize];
            if file.read_exact_at(&mut data, self.offset - last).is_ok() && data.starts_with(&self.last) {
                let records = parse(&data[last as usize..], self.offset == 0, true);
                let base = self.offset;
                self.seen(&data[last as usize..], base, &records);
                self.count += records.len();
                return records;
            }
        }
        // The file was replaced (or rewritten): find the last entry seen.
        self.id = id;
        let Some(data) = read_all(file) else { return Vec::new() };
        let all = parse(&data, true, true);
        let mut from = 0;
        if !self.last.is_empty() {
            let found = (data.windows(self.last.len()).enumerate().rev())
                .find(|&(i, w)| w == self.last.as_slice() && (i == 0 || data[i - 1] == b'\n'));
            from = found.map_or(data.len(), |(i, _)| i + self.last.len());
        }
        let split = all.iter().position(|r| r.end > from).unwrap_or(all.len());
        self.count = all.len();
        self.offset = data.len() as u64;
        self.seen(&data, 0, &all);
        // Entries before `from` were seen already.
        all.into_iter().skip(split).collect()
    }

    /// Records that the shell appended `data` (whole entries) to `file`,
    /// whose size was `size` before.
    fn appended(&mut self, size: u64, data: &[u8], count: usize) {
        let records = parse(data, false, true);
        self.seen(data, size, &records);
        self.count += count;
    }
}

/// An open history file, locked for writing.
pub struct Locked {
    pub file: File,
}

impl Locked {
    /// Opens the file (creating it, and its directory, if needed) and waits
    /// for the lock. The file may have been replaced by the time the lock
    /// is granted, in which case it starts again with the new one.
    pub fn open(path: &[u8]) -> Option<Locked> {
        let p = to_path(path);
        if let Some(dir) = p.parent()
            && !dir.as_os_str().is_empty()
        {
            let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
        }
        for _ in 0..10 {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .mode(0o600)
                .open(&p)
                .ok()?;
            // SAFETY: an all-zero flock is valid; the fields are set below.
            let mut lock: libc::flock = unsafe { std::mem::zeroed() };
            lock.l_type = libc::F_WRLCK as _;
            lock.l_whence = libc::SEEK_SET as _;
            loop {
                // SAFETY: fcntl on our open file with a valid flock.
                if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLKW, &lock) } == 0 {
                    break;
                }
                if sys::errno() != libc::EINTR {
                    return None;
                }
            }
            let same = (file.metadata().ok())
                .zip(sys::stat(path))
                .is_some_and(|(m, s)| m.ino() == s.st_ino && m.dev() == s.st_dev);
            if same {
                return Some(Locked { file });
            }
        }
        None
    }

    /// Appends whole entries.
    pub fn append(&mut self, st: &mut FileState, data: &[u8], count: usize) -> bool {
        let Ok(size) = self.file.metadata().map(|m| m.len()) else {
            return false;
        };
        if self.file.write_all(data).is_err() {
            return false;
        }
        if let Ok(m) = self.file.metadata() {
            st.id = (m.dev(), m.ino());
        }
        st.appended(size, data, count);
        true
    }

    /// Replaces the file with its last `limit` entries (without older
    /// duplicates with `no_dups`), written to a temporary file that is
    /// renamed over it.
    pub fn trim(self, st: &mut FileState, limit: usize, no_dups: bool) {
        let Some(data) = read_all(&self.file) else { return };
        let records = parse(&data, true, true);
        let mut keep: Vec<&Record> = Vec::new();
        let mut texts = std::collections::HashSet::new();
        for r in records.iter().rev() {
            if keep.len() >= limit {
                break;
            }
            if !no_dups || texts.insert(r.text.as_str()) {
                keep.push(r);
            }
        }
        let mut out = Vec::new();
        for r in keep.iter().rev() {
            format(&r.text, r.time, r.elapsed, &mut out);
        }
        let mut prefix = st.path.clone();
        prefix.push(b'.');
        let Ok((fd, tmp)) = sys::mkstemp(&prefix) else { return };
        let written = sys::write_all(fd, &out);
        sys::close(fd);
        if !written || std::fs::rename(to_path(&tmp), to_path(&st.path)).is_err() {
            sys::unlink(&tmp);
            return;
        }
        let file = File::open(to_path(&st.path)).ok();
        let records = parse(&out, false, true);
        *st = FileState::read(&st.path.clone(), file.as_ref(), &out, &records);
        // The lock is released when `self` is dropped, after the rename.
    }
}

/// The contents of a file.
pub fn read_all(file: &File) -> Option<Vec<u8>> {
    let mut data = vec![0; file.metadata().ok()?.len() as usize];
    file.read_exact_at(&mut data, 0).ok()?;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(records: &[Record]) -> Vec<&str> {
        records.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn zsh_format() {
        let data = b": 1790410075:3;cargo build\nls\n: 1790423251:0;for i in 1\\\ndo :\\\ndone\n\
                     : 12:0;echo \xe7\xbb\x83\xb9\n";
        let r = parse(data, true, false);
        assert_eq!(
            texts(&r),
            ["cargo build", "ls", "for i in 1\ndo :\ndone", "echo \u{7ed9}"]
        );
        assert_eq!((r[0].time, r[0].elapsed, r[1].time), (1790410075, 3, 0));
        let mut out = Vec::new();
        format(&r[0].text, r[0].time, r[0].elapsed, &mut out);
        format(&r[1].text, 0, 0, &mut out);
        for rec in &r[2..] {
            format(&rec.text, rec.time, rec.elapsed, &mut out);
        }
        assert_eq!(out, data);
    }

    #[test]
    fn old_format_and_backslashes() {
        let data = b"#V2\necho a\\\\\nfor i\\ndo :\n: 5:0;echo b\\\\\nc\n";
        let r = parse(data, true, false);
        assert_eq!(texts(&r), ["echo a\\", "for i\ndo :", "echo b\\\nc"]);
        let mut out = Vec::new();
        format(&r[2].text, 5, 0, &mut out);
        format(&r[0].text, 5, 0, &mut out);
        assert_eq!(out, b": 5:0;echo b\\\\\nc\n: 5:0;echo a\\ \n");
        assert_eq!(texts(&parse(&out, true, false)), ["echo b\\\nc", "echo a\\ "]);
    }

    #[test]
    fn incomplete_entries() {
        assert_eq!(texts(&parse(b"a\nb", true, true)), ["a"]);
        assert_eq!(texts(&parse(b"a\nb\\\n", true, true)), ["a"]);
        assert_eq!(texts(&parse(b"a\nb", true, false)), ["a", "b"]);
    }
}
