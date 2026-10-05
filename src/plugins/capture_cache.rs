//! The cache of `sh::capture_cached` (DEVELOPING.md): what a program
//! printed, kept in memory for a few seconds so that a completer that
//! scrapes `-h` doesn't run it again at each Tab. Nothing is promised:
//! an entry may be dropped at any time, and is when the user runs a
//! command that names its program (`invalidate`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The longest an entry is kept.
pub const MAX_TTL: u64 = 300;
/// A failure (a status other than 0) is kept this long, times the
/// number of failures in a row, so that a missing program isn't run at
/// each Tab, but one that is installed is soon found.
const FAILURE_STEP: Duration = Duration::from_secs(3);
/// The size of all entries together (outputs and keys), beyond which the
/// oldest are dropped.
const MAX_BYTES: usize = 32 << 20;
const MAX_ENTRIES: usize = 4096;

/// What a run depends on besides its program and arguments.
#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Key {
    pub argv: Vec<Vec<u8>>,
    /// Where standard error goes (`super::Stderr`).
    pub stderr: u8,
    pub dir: Vec<u8>,
    pub path: Vec<u8>,
}

impl Key {
    fn size(&self) -> usize {
        self.argv.iter().map(|w| w.len()).sum::<usize>() + self.dir.len() + self.path.len()
    }
}

pub struct Entry {
    pub status: i32,
    pub out: Vec<u8>,
    pub err: Vec<u8>,
    at: Instant,
    ttl: Duration,
    /// The failures in a row, 0 after a success.
    failures: u32,
}

impl Entry {
    fn size(&self) -> usize {
        self.out.len() + self.err.len()
    }

    fn fresh(&self, now: Instant) -> bool {
        now < self.at + self.ttl
    }

    /// Whether to drop the entry: a failure is kept as long again after it
    /// expires, so that a failure soon after continues its run.
    fn dead(&self, now: Instant) -> bool {
        let grace = if self.failures > 0 { self.ttl } else { Duration::ZERO };
        now >= self.at + self.ttl + grace
    }
}

#[derive(Default)]
pub struct CaptureCache {
    entries: HashMap<Key, Entry>,
    bytes: usize,
}

impl CaptureCache {
    /// The output of the run `key`, if it is fresh.
    pub fn get(&self, key: &Key, now: Instant) -> Option<&Entry> {
        self.entries.get(key).filter(|e| e.fresh(now))
    }

    /// Records the result of the run `key`, to keep for `ttl` seconds (at
    /// most [`MAX_TTL`]), or less if it failed.
    pub fn insert(&mut self, key: Key, ttl: u64, status: i32, out: Vec<u8>, err: Vec<u8>, now: Instant) {
        let failures = match (status, self.remove(&key)) {
            (0, _) => 0,
            (_, Some(prev)) if !prev.dead(now) => prev.failures.saturating_add(1).max(1),
            _ => 1,
        };
        let ttl = Duration::from_secs(ttl.min(MAX_TTL));
        let ttl = if failures > 0 {
            ttl.min(FAILURE_STEP.saturating_mul(failures))
        } else {
            ttl
        };
        let entry = Entry {
            status,
            out,
            err,
            at: now,
            ttl,
            failures,
        };
        let size = key.size() + entry.size();
        if ttl.is_zero() || size > MAX_BYTES {
            return;
        }
        self.entries.retain(|k, e| {
            let keep = !e.dead(now);
            if !keep {
                self.bytes -= k.size() + e.size();
            }
            keep
        });
        while self.bytes + size > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let Some(oldest) = self.entries.iter().min_by_key(|(_, e)| e.at).map(|(k, _)| k.clone()) else {
                break;
            };
            self.remove(&oldest);
        }
        self.bytes += size;
        self.entries.insert(key, entry);
    }

    fn remove(&mut self, key: &Key) -> Option<Entry> {
        let e = self.entries.remove(key)?;
        self.bytes -= key.size() + e.size();
        Some(e)
    }

    /// Drops the entries whose arguments name a program that the command
    /// line `line` runs (or any of its words that could be a program: an
    /// entry dropped for nothing is only run again), so that what
    /// `cargo install` or `rustup toolchain add` change is seen at once.
    /// The entries for `["env", "COLUMNS=400", "cargo", "-h"]` go too.
    pub fn invalidate(&mut self, line: &[u8]) {
        if self.entries.is_empty() {
            return;
        }
        let words: Vec<&[u8]> = line
            .split(|c| c.is_ascii_whitespace() || b";&|()<>`'\"".contains(c))
            .filter(|w| name_like(w))
            .map(basename)
            .collect();
        if words.is_empty() {
            return;
        }
        let gone: Vec<Key> = self
            .entries
            .keys()
            .filter(|k| k.argv.iter().any(|a| name_like(a) && words.contains(&basename(a))))
            .cloned()
            .collect();
        for k in gone {
            self.remove(&k);
        }
    }
}

/// Whether a word could name a program: not an option or an assignment.
fn name_like(w: &[u8]) -> bool {
    !w.is_empty() && w[0] != b'-' && !w.contains(&b'=')
}

fn basename(w: &[u8]) -> &[u8] {
    match w.iter().rposition(|&c| c == b'/') {
        Some(i) => &w[i + 1..],
        None => w,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(argv: &[&str]) -> Key {
        Key {
            argv: argv.iter().map(|w| w.as_bytes().to_vec()).collect(),
            stderr: 0,
            dir: b"/home".to_vec(),
            path: b"/bin".to_vec(),
        }
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn expires() {
        let t = Instant::now();
        let mut c = CaptureCache::default();
        c.insert(key(&["a"]), 10, 0, b"out".to_vec(), Vec::new(), t);
        assert_eq!(c.get(&key(&["a"]), t + secs(9)).unwrap().out, b"out");
        assert!(c.get(&key(&["a"]), t + secs(10)).is_none());
        assert!(c.get(&key(&["b"]), t).is_none());
        let mut other = key(&["a"]);
        other.dir = b"/tmp".to_vec();
        assert!(c.get(&other, t).is_none());
        // At most MAX_TTL; 0 isn't kept.
        c.insert(key(&["long"]), 100_000, 0, Vec::new(), Vec::new(), t);
        assert!(c.get(&key(&["long"]), t + secs(MAX_TTL - 1)).is_some());
        assert!(c.get(&key(&["long"]), t + secs(MAX_TTL)).is_none());
        c.insert(key(&["zero"]), 0, 0, Vec::new(), Vec::new(), t);
        assert!(c.get(&key(&["zero"]), t).is_none());
    }

    #[test]
    fn failures_back_off() {
        let t = Instant::now();
        let mut c = CaptureCache::default();
        let k = || key(&["missing"]);
        c.insert(k(), 60, 127, Vec::new(), Vec::new(), t);
        assert!(c.get(&k(), t + secs(2)).is_some());
        assert!(c.get(&k(), t + secs(3)).is_none());
        // Failing again soon after: 6 s, then 9 s.
        let t = t + secs(4);
        c.insert(k(), 60, 127, Vec::new(), Vec::new(), t);
        assert!(c.get(&k(), t + secs(5)).is_some());
        assert!(c.get(&k(), t + secs(6)).is_none());
        let t = t + secs(6);
        c.insert(k(), 60, 127, Vec::new(), Vec::new(), t);
        assert!(c.get(&k(), t + secs(8)).is_some());
        assert!(c.get(&k(), t + secs(9)).is_none());
        // Never longer than asked.
        c.insert(k(), 4, 1, Vec::new(), Vec::new(), t + secs(9));
        assert!(c.get(&k(), t + secs(9 + 4)).is_none());
        // Much later, a failure starts again at 3 s.
        let t = t + secs(1000);
        c.insert(k(), 60, 127, Vec::new(), Vec::new(), t);
        assert!(c.get(&k(), t + secs(3)).is_none());
        // A success is kept for the whole time.
        c.insert(k(), 60, 0, Vec::new(), Vec::new(), t);
        assert!(c.get(&k(), t + secs(59)).is_some());
    }

    #[test]
    fn bounded() {
        let t = Instant::now();
        let mut c = CaptureCache::default();
        let big = MAX_BYTES / 3;
        for (i, name) in ["a", "b", "c", "d"].iter().enumerate() {
            c.insert(key(&[name]), 60, 0, vec![b'x'; big], Vec::new(), t + secs(i as u64));
        }
        assert!(c.bytes <= MAX_BYTES);
        assert!(c.get(&key(&["a"]), t + secs(5)).is_none());
        assert!(c.get(&key(&["d"]), t + secs(5)).is_some());
        // Expired entries go when another is added.
        c.insert(key(&["e"]), 60, 0, Vec::new(), Vec::new(), t + secs(100));
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.bytes, key(&["e"]).size());
        // An output too big for the cache isn't kept.
        c.insert(key(&["f"]), 60, 0, vec![b'x'; MAX_BYTES + 1], Vec::new(), t + secs(100));
        assert!(c.get(&key(&["f"]), t + secs(100)).is_none());
    }

    #[test]
    fn invalidates() {
        let t = Instant::now();
        let mut c = CaptureCache::default();
        for argv in [
            &["env", "COLUMNS=400", "cargo", "-h"][..],
            &["/usr/bin/rustup", "toolchain", "list"],
            &["git", "branch"],
        ] {
            c.insert(key(argv), 60, 0, Vec::new(), Vec::new(), t);
        }
        // Options and assignments name no program.
        c.invalidate(b"ls -h COLUMNS=1");
        assert_eq!(c.entries.len(), 3);
        c.invalidate(b"FOO=1 ~/.cargo/bin/cargo install x && echo done");
        assert!(c.get(&key(&["env", "COLUMNS=400", "cargo", "-h"]), t).is_none());
        c.invalidate(b"(rustup update)");
        assert!(c.get(&key(&["/usr/bin/rustup", "toolchain", "list"]), t).is_none());
        assert!(c.get(&key(&["git", "branch"]), t).is_some());
        assert_eq!(c.bytes, key(&["git", "branch"]).size());
    }
}
