//! The command history: what the line editor recalls and what `fc` lists
//! and re-runs.
//!
//! This owns the storage (and the `$HISTFILE` format) rather than using
//! rustyline's `FileHistory`, because `fc` needs event numbers that stay
//! the same when old entries are dropped, and must be able to replace the
//! entry of the `fc` command itself. The file format is rustyline's: a
//! `#V2` line, then one entry per line with `\` and newline escaped.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;

use rustyline::history::{History, SearchDirection, SearchResult};

pub struct ShellHistory {
    entries: VecDeque<String>,
    /// The event number of `entries[0]`.
    first: usize,
    max_len: usize,
    ignore_dups: bool,
    ignore_space: bool,
    /// The newest entry is the command being run (so `fc` skips it).
    current: bool,
    /// Something changed since the file was loaded or saved.
    changed: bool,
}

impl Default for ShellHistory {
    fn default() -> Self {
        ShellHistory {
            entries: VecDeque::new(),
            first: 1,
            max_len: 1000,
            ignore_dups: true,
            ignore_space: false,
            current: false,
            changed: false,
        }
    }
}

const FILE_VERSION_V2: &str = "#V2";

impl ShellHistory {
    /// The event numbers of the oldest and newest entries, leaving out the
    /// command being run, if any.
    pub fn range(&self) -> Option<(usize, usize)> {
        let n = self.entries.len() - usize::from(self.current && !self.entries.is_empty());
        (n > 0).then(|| (self.first, self.first + n - 1))
    }

    /// Adds the text of a command about to be run. It counts as current
    /// even when it is not added because it repeats the newest entry.
    pub fn add_current(&mut self, line: &str) {
        let added = self.add(line).unwrap_or(false);
        self.current = added || self.entries.back().is_some_and(|s| s == line);
    }

    /// Removes the entry of the command being run: `fc` replaces its own
    /// entry with the commands it runs.
    pub fn remove_current(&mut self) {
        if std::mem::take(&mut self.current) {
            self.entries.pop_back();
            self.changed = true;
        }
    }

    /// The event number the next entry will get.
    pub fn next_event(&self) -> usize {
        self.first + self.entries.len()
    }

    /// The entry with event number `n`.
    pub fn event(&self, n: usize) -> Option<&str> {
        self.entries.get(n.checked_sub(self.first)?).map(String::as_str)
    }

    fn ignore(&self, line: &str) -> bool {
        self.max_len == 0
            || line.is_empty()
            || (self.ignore_space && line.starts_with(char::is_whitespace))
            || (self.ignore_dups && self.entries.back().is_some_and(|s| s == line))
    }

    fn search_match(
        &self,
        start: usize,
        dir: SearchDirection,
        test: impl Fn(&str) -> Option<usize>,
    ) -> Option<SearchResult<'_>> {
        let found = |idx: usize| {
            let entry = &self.entries[idx];
            test(entry).map(|pos| SearchResult {
                entry: Cow::Borrowed(entry.as_str()),
                idx,
                pos,
            })
        };
        if start >= self.entries.len() {
            return None;
        }
        match dir {
            SearchDirection::Reverse => (0..=start).rev().find_map(found),
            SearchDirection::Forward => (start..self.entries.len()).find_map(found),
        }
    }
}

fn escape(entry: &str, out: &mut Vec<u8>) {
    for &b in entry.as_bytes() {
        match b {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            _ => out.push(b),
        }
    }
    out.push(b'\n');
}

fn unescape(line: &str) -> String {
    let mut s = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => s.push('\n'),
                Some(c) => s.push(c),
                None => s.push('\\'),
            }
        } else {
            s.push(c);
        }
    }
    s
}

fn io_err(e: std::io::Error) -> rustyline::error::ReadlineError {
    rustyline::error::ReadlineError::Io(e)
}

impl History for ShellHistory {
    fn get(&self, index: usize, _: SearchDirection) -> rustyline::Result<Option<SearchResult<'_>>> {
        Ok(self.entries.get(index).map(|e| SearchResult {
            entry: Cow::Borrowed(e.as_str()),
            idx: index,
            pos: 0,
        }))
    }

    fn add(&mut self, line: &str) -> rustyline::Result<bool> {
        self.add_owned(line.to_owned())
    }

    fn add_owned(&mut self, line: String) -> rustyline::Result<bool> {
        if self.ignore(&line) {
            return Ok(false);
        }
        if self.entries.len() >= self.max_len {
            self.entries.pop_front();
            self.first += 1;
        }
        self.entries.push_back(line);
        self.changed = true;
        Ok(true)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn set_max_len(&mut self, len: usize) -> rustyline::Result<()> {
        self.max_len = len;
        let excess = self.entries.len().saturating_sub(len);
        self.entries.drain(..excess);
        self.first += excess;
        Ok(())
    }

    fn ignore_dups(&mut self, yes: bool) -> rustyline::Result<()> {
        self.ignore_dups = yes;
        Ok(())
    }

    fn ignore_space(&mut self, yes: bool) {
        self.ignore_space = yes;
    }

    fn save(&mut self, path: &Path) -> rustyline::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if !self.changed {
            return Ok(());
        }
        let mut out = Vec::new();
        out.extend_from_slice(FILE_VERSION_V2.as_bytes());
        out.push(b'\n');
        for e in &self.entries {
            escape(e, &mut out);
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(io_err)?;
        f.write_all(&out).map_err(io_err)?;
        self.changed = false;
        Ok(())
    }

    fn append(&mut self, path: &Path) -> rustyline::Result<()> {
        self.save(path)
    }

    fn load(&mut self, path: &Path) -> rustyline::Result<()> {
        let data = std::fs::read(path).map_err(io_err)?;
        let text = String::from_utf8_lossy(&data);
        let mut lines = text.lines().peekable();
        let v2 = lines.next_if_eq(&FILE_VERSION_V2).is_some();
        for line in lines.filter(|l| !l.is_empty()) {
            self.add_owned(if v2 { unescape(line) } else { line.to_owned() })?;
        }
        self.changed = false;
        Ok(())
    }

    fn clear(&mut self) -> rustyline::Result<()> {
        self.first += self.entries.len();
        self.entries.clear();
        self.current = false;
        self.changed = true;
        Ok(())
    }

    fn search(&self, term: &str, start: usize, dir: SearchDirection) -> rustyline::Result<Option<SearchResult<'_>>> {
        if term.is_empty() {
            return Ok(None);
        }
        Ok(self.search_match(start, dir, |e| e.find(term)))
    }

    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> rustyline::Result<Option<SearchResult<'_>>> {
        if term.is_empty() {
            return Ok(None);
        }
        Ok(self.search_match(start, dir, |e| e.starts_with(term).then_some(term.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_survive_dropping_old_entries() {
        let mut h = ShellHistory::default();
        h.set_max_len(2).unwrap();
        for l in ["a", "b", "c"] {
            h.add(l).unwrap();
        }
        assert_eq!(h.range(), Some((2, 3)));
        assert_eq!(h.event(2), Some("b"));
        assert_eq!(h.event(1), None);
        h.add_current("c");
        assert_eq!(h.range(), Some((2, 2)));
        h.remove_current();
        assert_eq!(h.range(), Some((2, 2)));
        assert_eq!(h.event(3), None);
    }

    #[test]
    fn file_round_trip() {
        let dir = std::env::temp_dir().join(format!("luish-hist-{}", std::process::id()));
        let mut h = ShellHistory::default();
        for l in ["echo a\\b", "for i in 1\ndo :\ndone", "x"] {
            h.add(l).unwrap();
        }
        h.save(&dir).unwrap();
        let mut g = ShellHistory::default();
        g.load(&dir).unwrap();
        let _ = std::fs::remove_file(&dir);
        assert_eq!(g.entries, h.entries);
    }

    #[test]
    fn search() {
        let mut h = ShellHistory::default();
        for l in ["echo one", "ls", "echo two"] {
            h.add(l).unwrap();
        }
        let r = h.starts_with("echo", 1, SearchDirection::Reverse).unwrap().unwrap();
        assert_eq!(r.idx, 0);
        let r = h.search("two", 0, SearchDirection::Forward).unwrap().unwrap();
        assert_eq!((r.idx, r.pos), (2, 5));
    }
}
