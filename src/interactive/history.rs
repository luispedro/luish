//! The command history: what the line editor recalls and what `fc` lists
//! and re-runs.
//!
//! This owns the storage rather than using rustyline's `FileHistory`,
//! because `fc` needs event numbers that stay the same when old entries are
//! dropped, and must be able to replace the entry of the `fc` command
//! itself. The file is zsh's (see `histfile.rs`).

use std::borrow::Cow;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rustyline::history::{History, SearchDirection, SearchResult};

use super::histfile::{self, FileState, Locked, Record};

struct Entry {
    text: String,
    /// When the command was run, in seconds since the epoch (0 if unknown).
    time: i64,
}

pub struct ShellHistory {
    entries: VecDeque<Entry>,
    /// The event number of `entries[0]`.
    first: usize,
    max_len: usize,
    ignore_dups: bool,
    /// The newest entry is the command being run (so `fc` skips it).
    current: bool,
    /// The newest entry is not to be saved, and is replaced by the next
    /// one (`hist_ignore_space`).
    private: bool,
    /// The event number of the first entry not in the file.
    saved: usize,
    file: FileState,
    /// Shared with the key bindings, for the prefix searches (`starts_with`).
    pub search: Arc<Mutex<Search>>,
}

/// The line being edited, for zsh's `history-beginning-search-backward`
/// and `-forward` (see `keys.rs`), which rustyline's anchored search
/// (`starts_with`) implements.
#[derive(Default)]
pub struct Search {
    /// The line when the key was pressed: entries equal to it are skipped.
    pub current: String,
    /// The line as typed, before the searches: going forward past the
    /// newest match brings it back.
    pub original: String,
    /// The entry the last search found.
    pub shown: Option<String>,
}

impl Default for ShellHistory {
    fn default() -> Self {
        ShellHistory {
            entries: VecDeque::new(),
            first: 1,
            max_len: 1000,
            ignore_dups: true,
            current: false,
            private: false,
            saved: 1,
            file: FileState::default(),
            search: Arc::default(),
        }
    }
}

/// How the history is written to its file.
pub struct Save<'a> {
    pub path: &'a [u8],
    /// The number of entries to keep in the file.
    pub limit: usize,
    /// Read the entries other shells have added (`share_history`).
    pub share: bool,
    /// Leave out older duplicates when the file is trimmed.
    pub no_dups: bool,
}

impl ShellHistory {
    /// The event numbers of the oldest and newest entries, leaving out the
    /// command being run, if any.
    pub fn range(&self) -> Option<(usize, usize)> {
        let n = self.entries.len() - usize::from(self.current && !self.entries.is_empty());
        (n > 0).then(|| (self.first, self.first + n - 1))
    }

    /// Adds the text of a command about to be run. It counts as current
    /// even when it is not added because it repeats the newest entry. A
    /// `private` one isn't saved, and the next entry replaces it.
    pub fn add_current(&mut self, line: &str, private: bool) {
        if std::mem::take(&mut self.private) {
            self.pop();
        }
        let added = self.push(line.to_owned(), crate::sys::now());
        self.current = added || self.entries.back().is_some_and(|e| e.text == line);
        self.private = added && private;
    }

    /// Adds an entry after the command being run (`print -s`), which is
    /// then no longer the newest.
    pub fn add_entry(&mut self, line: &str) {
        if std::mem::take(&mut self.private) {
            self.pop();
            self.current = false;
        }
        if self.push(line.to_owned(), crate::sys::now()) {
            self.current = false;
        }
    }

    /// Removes the entry of the command being run: `fc` replaces its own
    /// entry with the commands it runs.
    pub fn remove_current(&mut self) {
        if std::mem::take(&mut self.current) {
            self.pop();
            self.private = false;
        }
    }

    fn pop(&mut self) {
        self.entries.pop_back();
        self.saved = self.saved.min(self.next_event());
    }

    /// The event number the next entry will get.
    pub fn next_event(&self) -> usize {
        self.first + self.entries.len()
    }

    /// `$HISTCMD`: the event number of the command being run, or else of
    /// the next one.
    pub fn current_event(&self) -> usize {
        self.next_event() - usize::from(self.current)
    }

    /// The event number of the entry at `index` (as rustyline counts them,
    /// from the oldest kept).
    pub fn event_at(&self, index: usize) -> usize {
        self.first + index
    }

    /// The index of the entry with event number `n`.
    pub fn index_of(&self, n: usize) -> Option<usize> {
        n.checked_sub(self.first).filter(|&i| i < self.entries.len())
    }

    /// The entry with event number `n`.
    pub fn event(&self, n: usize) -> Option<&str> {
        let e = self.entries.get(n.checked_sub(self.first)?)?;
        Some(&e.text)
    }

    fn ignore(&self, line: &str) -> bool {
        self.max_len == 0
            || line.is_empty()
            || (self.ignore_dups && self.entries.back().is_some_and(|e| e.text == line))
    }

    /// Adds an entry, unless it is ignored. Returns whether it was added.
    fn push(&mut self, text: String, time: i64) -> bool {
        if self.ignore(&text) {
            return false;
        }
        if self.entries.len() >= self.max_len {
            self.entries.pop_front();
            self.first += 1;
        }
        self.entries.push_back(Entry { text, time });
        true
    }

    /// Adds entries read from the file before those not saved yet.
    fn import(&mut self, records: Vec<Record>) {
        if records.is_empty() {
            return;
        }
        let unsaved = self.next_event().saturating_sub(self.saved.max(self.first));
        let mut kept: Vec<Entry> = (0..unsaved).filter_map(|_| self.entries.pop_back()).collect();
        // The current command stays the newest entry only if it is unsaved.
        self.current &= unsaved > 0;
        for r in records {
            self.push(r.text, r.time);
        }
        self.saved = self.next_event();
        while let Some(e) = kept.pop() {
            self.entries.push_back(e);
        }
        let excess = self.entries.len().saturating_sub(self.max_len);
        self.entries.drain(..excess);
        self.first += excess;
    }

    /// Reads the history file.
    pub fn load(&mut self, path: &[u8]) {
        let file = std::fs::File::open(super::to_path(path)).ok();
        let data = file.as_ref().and_then(histfile::read_all).unwrap_or_default();
        let records = histfile::parse(&data, true, false);
        self.file = FileState::read(path, file.as_ref(), &data, &records);
        for r in records {
            self.push(r.text, r.time);
        }
        self.saved = self.next_event();
    }

    /// Starts again with a file other than the one loaded (`HISTFILE` was
    /// changed).
    fn use_file(&mut self, path: &[u8]) {
        if self.file.path != path {
            self.file = FileState::new(path);
        }
    }

    /// Adds the entries that other shells have written to the file since
    /// the shell last read or wrote it (`share_history`).
    pub fn sync(&mut self, path: &[u8]) {
        self.use_file(path);
        if !self.file.changed() {
            return;
        }
        if let Ok(f) = std::fs::File::open(super::to_path(path)) {
            let records = self.file.new_records(&f);
            self.import(records);
        }
    }

    /// Appends the entries not saved yet to the file, then trims it if it
    /// has grown to more than 20% over its limit (as zsh does).
    pub fn save(&mut self, save: &Save) {
        let end = self.next_event() - usize::from(self.private);
        if self.saved >= end {
            return;
        }
        let Some(mut locked) = Locked::open(save.path) else {
            return;
        };
        self.use_file(save.path);
        // Keep track of the file even when not sharing it, so that turning
        // `share_history` on later reads only what is new.
        let records = self.file.new_records(&locked.file);
        if save.share {
            self.import(records);
        }
        let end = self.next_event() - usize::from(self.private);
        let mut out = Vec::new();
        let start = self.saved.max(self.first);
        for n in start..end {
            let e = &self.entries[n - self.first];
            histfile::format(&e.text, e.time, 0, &mut out);
        }
        if locked.append(&mut self.file, &out, end - start) {
            self.saved = end;
        }
        if self.file.count > save.limit + save.limit / 5 {
            locked.trim(&mut self.file, save.limit, save.no_dups);
        }
    }

    fn search_match(
        &self,
        start: usize,
        dir: SearchDirection,
        test: impl Fn(&str) -> Option<usize>,
    ) -> Option<SearchResult<'_>> {
        let found = |idx: usize| {
            let entry = &self.entries[idx].text;
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

/// Removes superfluous blanks from a command, as zsh's `hist_reduce_blanks`
/// does: runs of spaces and tabs become one space, and those at the start
/// and end of lines go, except in quotes and in here-documents.
pub fn reduce_blanks(text: &str) -> String {
    #[derive(PartialEq)]
    enum Q {
        Single,
        Double,
        Backtick,
    }
    let mut out = String::with_capacity(text.len());
    let mut quotes: Vec<Q> = Vec::new();
    let mut heredoc = false;
    let mut blank = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let top = quotes.last();
        if top.is_none() && (c == ' ' || c == '\t') {
            blank = true;
            continue;
        }
        if blank {
            if !out.is_empty() && !out.ends_with('\n') && c != '\n' {
                out.push(' ');
            }
            blank = false;
        }
        out.push(c);
        match (top, c) {
            (Some(Q::Single), '\'') => {
                quotes.pop();
            }
            (Some(Q::Single), _) => {}
            (_, '\\') => out.extend(chars.next().map(|(_, c)| c)),
            (Some(Q::Double), '"') | (Some(Q::Backtick), '`') => {
                quotes.pop();
            }
            (None, '\'') => quotes.push(Q::Single),
            (None | Some(Q::Backtick), '"') => quotes.push(Q::Double),
            (None | Some(Q::Double), '`') => quotes.push(Q::Backtick),
            (None, '<') if text[i..].starts_with("<<") => heredoc = true,
            (None, '\n') if heredoc => {
                // The rest may be here-document text: keep it as it is.
                out.extend(chars.map(|(_, c)| c));
                break;
            }
            _ => {}
        }
    }
    out
}

impl History for ShellHistory {
    fn get(&self, index: usize, _: SearchDirection) -> rustyline::Result<Option<SearchResult<'_>>> {
        Ok(self.entries.get(index).map(|e| SearchResult {
            entry: Cow::Borrowed(e.text.as_str()),
            idx: index,
            pos: 0,
        }))
    }

    fn add(&mut self, line: &str) -> rustyline::Result<bool> {
        self.add_owned(line.to_owned())
    }

    fn add_owned(&mut self, line: String) -> rustyline::Result<bool> {
        Ok(self.push(line, crate::sys::now()))
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

    /// `hist_ignore_space` is given to `add_current` instead.
    fn ignore_space(&mut self, _: bool) {}

    fn save(&mut self, path: &Path) -> rustyline::Result<()> {
        self.append(path)
    }

    fn append(&mut self, path: &Path) -> rustyline::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let limit = self.max_len;
        ShellHistory::save(
            self,
            &Save {
                path: path.as_os_str().as_bytes(),
                limit,
                share: false,
                no_dups: false,
            },
        );
        Ok(())
    }

    fn load(&mut self, path: &Path) -> rustyline::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        ShellHistory::load(self, path.as_os_str().as_bytes());
        Ok(())
    }

    fn clear(&mut self) -> rustyline::Result<()> {
        self.first += self.entries.len();
        self.entries.clear();
        self.current = false;
        self.private = false;
        self.saved = self.first;
        Ok(())
    }

    fn search(&self, term: &str, start: usize, dir: SearchDirection) -> rustyline::Result<Option<SearchResult<'_>>> {
        if term.is_empty() {
            return Ok(None);
        }
        Ok(self.search_match(start, dir, |e| e.find(term)))
    }

    /// The entries that start with `term` (all of them if it is empty),
    /// other than the line being edited. Going forward past the newest
    /// match gives back the line as it was typed, as zsh's
    /// `history-beginning-search-forward` does.
    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> rustyline::Result<Option<SearchResult<'_>>> {
        let Ok(mut search) = self.search.lock() else {
            return Ok(None);
        };
        let current = std::mem::take(&mut search.current);
        let found = self.search_match(start, dir, |e| {
            (e.starts_with(term) && e != current).then_some(term.len())
        });
        let found = match found {
            None if dir == SearchDirection::Forward
                && search.original != current
                && search.original.starts_with(term) =>
            {
                Some(SearchResult {
                    entry: Cow::Owned(search.original.clone()),
                    idx: self.entries.len(),
                    pos: term.len(),
                })
            }
            found => found,
        };
        // When nothing is found, the line stays as it was.
        if let Some(f) = &found {
            search.shown = Some(f.entry.to_string());
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(h: &ShellHistory) -> Vec<&str> {
        h.entries.iter().map(|e| e.text.as_str()).collect()
    }

    /// A temporary file name, removed when dropped.
    struct Temp(std::path::PathBuf);

    impl Temp {
        fn new(name: &str) -> Temp {
            let dir = std::env::temp_dir().join(format!("luish-hist-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Temp(dir)
        }

        fn file(&self) -> Vec<u8> {
            self.0.join("history").into_os_string().into_encoded_bytes()
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn save(path: &[u8], share: bool) -> Save<'_> {
        Save {
            path,
            limit: 10,
            share,
            no_dups: false,
        }
    }

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
        h.add_current("c", false);
        assert_eq!(h.range(), Some((2, 2)));
        h.remove_current();
        assert_eq!(h.range(), Some((2, 2)));
        assert_eq!(h.event(3), None);
    }

    #[test]
    fn file_round_trip() {
        let t = Temp::new("round");
        let path = t.file();
        let mut h = ShellHistory::default();
        for l in ["echo a\\b", "for i in 1\ndo :\ndone", "x"] {
            h.add_current(l, false);
        }
        h.save(&save(&path, false));
        let mut g = ShellHistory::default();
        g.load(&path);
        assert_eq!(texts(&g), texts(&h));
        // Only new entries are appended.
        g.add_current("y", false);
        g.save(&save(&path, false));
        h.load(&path);
        assert_eq!(
            texts(&h),
            [
                "echo a\\b",
                "for i in 1\ndo :\ndone",
                "x",
                "echo a\\b",
                "for i in 1\ndo :\ndone",
                "x",
                "y"
            ]
        );
    }

    #[test]
    fn private_entries() {
        let t = Temp::new("private");
        let path = t.file();
        let mut h = ShellHistory::default();
        h.add_current("a", false);
        h.add_current(" secret", true);
        assert_eq!(texts(&h), ["a", " secret"]);
        h.save(&save(&path, false));
        h.add_current("b", false);
        assert_eq!(texts(&h), ["a", "b"]);
        assert_eq!(h.range(), Some((1, 1)));
        h.save(&save(&path, false));
        let mut g = ShellHistory::default();
        g.load(&path);
        assert_eq!(texts(&g), ["a", "b"]);
    }

    #[test]
    fn sharing() {
        let t = Temp::new("share");
        let path = t.file();
        let (mut a, mut b) = (ShellHistory::default(), ShellHistory::default());
        a.load(&path);
        b.load(&path);
        a.add_current("one", false);
        a.save(&save(&path, true));
        b.add_current("two", false);
        b.save(&save(&path, true));
        // `two` stays the newest entry of b, as the command being run.
        assert_eq!(texts(&b), ["one", "two"]);
        assert_eq!(b.range(), Some((1, 1)));
        a.sync(&path);
        assert_eq!(texts(&a), ["one", "two"]);
        a.sync(&path);
        assert_eq!(texts(&a), ["one", "two"]);
        // Replacing the file (as trimming does) keeps what was seen.
        let mut c = ShellHistory::default();
        c.load(&path);
        c.add_current("three", false);
        c.save(&Save {
            limit: 2,
            ..save(&path, true)
        });
        b.sync(&path);
        assert_eq!(texts(&b), ["one", "two", "three"]);
    }

    #[test]
    fn trimming() {
        let t = Temp::new("trim");
        let path = t.file();
        let mut h = ShellHistory::default();
        for l in ["a", "b", "a", "c", "b"] {
            h.add_current(l, false);
        }
        let s = Save {
            limit: 3,
            no_dups: true,
            ..save(&path, false)
        };
        h.save(&s);
        let mut g = ShellHistory::default();
        g.load(&path);
        assert_eq!(texts(&g), ["a", "c", "b"]);
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

    #[test]
    fn prefix_search() {
        let mut h = ShellHistory::default();
        for l in ["echo one", "ls", "echo two", "echo one"] {
            h.add(l).unwrap();
        }
        let press = |h: &ShellHistory, line: &str| {
            let mut s = h.search.lock().unwrap();
            if s.shown.as_deref() != Some(line) {
                s.original = line.to_owned();
            }
            s.current = line.to_owned();
        };
        let find = |h: &ShellHistory, term: &str, start: usize, dir| {
            let r = h.starts_with(term, start, dir).unwrap()?;
            Some((r.entry.into_owned(), r.idx, r.pos))
        };
        // Typed "echo", then Up: the newest match, the cursor after "echo".
        press(&h, "echo");
        assert_eq!(
            find(&h, "echo", 3, SearchDirection::Reverse),
            Some(("echo one".into(), 3, 4))
        );
        press(&h, "echo one");
        assert_eq!(
            find(&h, "echo", 2, SearchDirection::Reverse),
            Some(("echo two".into(), 2, 4))
        );
        // Up again skips "echo one", which is what the line was.
        press(&h, "echo two");
        assert_eq!(
            find(&h, "echo", 1, SearchDirection::Reverse),
            Some(("echo one".into(), 0, 4))
        );
        press(&h, "echo one");
        assert_eq!(find(&h, "echo", 0, SearchDirection::Reverse), None);
        // Down past the newest match brings back the line as typed.
        press(&h, "echo one");
        assert_eq!(
            find(&h, "echo", 1, SearchDirection::Forward),
            Some(("echo two".into(), 2, 4))
        );
        press(&h, "echo two");
        assert_eq!(
            find(&h, "echo", 3, SearchDirection::Forward),
            Some(("echo one".into(), 3, 4))
        );
        press(&h, "echo one");
        assert_eq!(
            find(&h, "echo", 4, SearchDirection::Forward),
            Some(("echo".into(), 4, 4))
        );
        // An empty prefix matches every entry.
        press(&h, "");
        assert_eq!(
            find(&h, "", 3, SearchDirection::Reverse),
            Some(("echo one".into(), 3, 0))
        );
    }

    #[test]
    fn blanks() {
        assert_eq!(reduce_blanks("  ls   -l\t x  "), "ls -l x");
        assert_eq!(
            reduce_blanks("echo 'a  b' \"c  $(d)\"   e\\  f"),
            "echo 'a  b' \"c  $(d)\" e\\  f"
        );
        assert_eq!(reduce_blanks("echo `a  \"b  c\"`  d"), "echo `a  \"b  c\"` d");
        assert_eq!(reduce_blanks("for i  in 1 \n  do :\ndone  "), "for i in 1\ndo :\ndone");
        assert_eq!(reduce_blanks("cat <<E  |  wc\n  a  b\nE"), "cat <<E | wc\n  a  b\nE");
    }
}
