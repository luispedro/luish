//! The cache of `sh::cache_get` and `sh::cache_put` (DEVELOPING.md): values
//! that extension code computed, kept in memory by plugin and key until
//! their time is up or a file they were computed from changes, so that a
//! completer doesn't parse the same text again at each Tab. As with
//! `capture_cache.rs`, nothing is promised: an entry may be dropped at any
//! time.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rhai::Dynamic;

/// The size of all entries together (estimated by [`size`]), beyond which
/// the oldest are dropped.
const MAX_BYTES: usize = 32 << 20;
const MAX_ENTRIES: usize = 4096;
/// What a value counts for besides its text.
const ELEMENT: usize = 16;
/// How deep [`size`] looks into nested arrays and maps.
const MAX_DEPTH: u32 = 64;
/// How many misses are remembered for the values not yet put.
const MAX_MISSES: usize = 256;

/// What a file was when a value was computed from it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stamp {
    dev: libc::dev_t,
    ino: libc::ino_t,
    size: libc::off_t,
    mtime: (libc::time_t, i64),
}

/// The stamp of the file at `path`, or `None` if there is none (a file
/// that is still missing is unchanged).
pub fn stamp(path: &[u8]) -> Option<Stamp> {
    crate::sys::stat(path).map(|st| Stamp {
        dev: st.st_dev,
        ino: st.st_ino,
        size: st.st_size,
        mtime: (st.st_mtime, st.st_mtime_nsec),
    })
}

struct Entry {
    value: Dynamic,
    files: Vec<(Vec<u8>, Option<Stamp>)>,
    at: Instant,
    ttl: Duration,
    size: usize,
}

impl Entry {
    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) >= self.ttl
    }
}

type Files = Vec<(Vec<u8>, Option<Stamp>)>;

/// `files`, each with its stamp.
fn stamped(files: Vec<Vec<u8>>, stamp: impl Fn(&[u8]) -> Option<Stamp>) -> Files {
    files
        .into_iter()
        .map(|f| {
            let s = stamp(&f);
            (f, s)
        })
        .collect()
}

#[derive(Default)]
pub struct ValueCache {
    /// By plugin, then by key.
    entries: HashMap<u32, HashMap<String, Entry>>,
    len: usize,
    bytes: usize,
    /// The files as they were when a key was missed, by plugin and key,
    /// until its value is put: a change while it is computed is then seen.
    misses: HashMap<(u32, String), Files>,
}

impl ValueCache {
    /// A copy of the value kept for `key` by `plugin`, if it is fresh and
    /// was computed from the same `files`, which `stamp` finds unchanged.
    pub fn get(
        &self,
        plugin: u32,
        key: &str,
        files: &[Vec<u8>],
        now: Instant,
        stamp: impl Fn(&[u8]) -> Option<Stamp>,
    ) -> Option<Dynamic> {
        let e = self.entries.get(&plugin)?.get(key)?;
        let same =
            e.files.len() == files.len() && e.files.iter().zip(files).all(|((p, s), f)| p == f && stamp(p) == *s);
        (same && !e.expired(now)).then(|| e.value.clone())
    }

    /// Notes that `key` was missed, with `files` as `stamp` finds them now.
    pub fn missed(&mut self, plugin: u32, key: &str, files: Vec<Vec<u8>>, stamp: impl Fn(&[u8]) -> Option<Stamp>) {
        if files.is_empty() {
            return;
        }
        if self.misses.len() >= MAX_MISSES {
            self.misses.clear();
        }
        self.misses.insert((plugin, key.into()), stamped(files, stamp));
    }

    /// The stamps of `files` for a value of `key`: as they were at the last
    /// miss of `key`, if it was for the same files, or as `stamp` finds them
    /// now.
    pub fn stamps(
        &mut self,
        plugin: u32,
        key: &str,
        files: Vec<Vec<u8>>,
        stamp: impl Fn(&[u8]) -> Option<Stamp>,
    ) -> Files {
        if let Some(m) = self.misses.remove(&(plugin, key.into()))
            && m.len() == files.len()
            && m.iter().zip(&files).all(|(a, b)| a.0 == *b)
        {
            return m;
        }
        stamped(files, stamp)
    }

    /// Keeps `value`, computed at `now` from `files` (as they were then),
    /// for `ttl` seconds.
    pub fn insert(&mut self, plugin: u32, key: String, ttl: u64, files: Files, value: Dynamic, now: Instant) {
        self.remove(plugin, &key);
        let size = key.len() + files.iter().map(|f| f.0.len() + ELEMENT).sum::<usize>() + self::size(&value, 0);
        if ttl == 0 || size > MAX_BYTES {
            return;
        }
        for map in self.entries.values_mut() {
            map.retain(|_, e| {
                let keep = !e.expired(now);
                if !keep {
                    self.len -= 1;
                    self.bytes -= e.size;
                }
                keep
            });
        }
        while self.bytes + size > MAX_BYTES || self.len >= MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .flat_map(|(p, map)| map.iter().map(move |(k, e)| (e.at, *p, k)))
                .min()
                .map(|(_, p, k)| (p, k.clone()));
            let Some((p, k)) = oldest else {
                break;
            };
            self.remove(p, &k);
        }
        self.len += 1;
        self.bytes += size;
        let entry = Entry {
            value,
            files,
            at: now,
            ttl: Duration::from_secs(ttl),
            size,
        };
        self.entries.entry(plugin).or_default().insert(key, entry);
    }

    fn remove(&mut self, plugin: u32, key: &str) {
        if let Some(e) = self.entries.get_mut(&plugin).and_then(|m| m.remove(key)) {
            self.len -= 1;
            self.bytes -= e.size;
        }
    }

    /// Drops what `plugin` kept, when it is unloaded (or loaded again).
    pub fn remove_plugin(&mut self, plugin: u32) {
        self.misses.retain(|k, _| k.0 != plugin);
        if let Some(map) = self.entries.remove(&plugin) {
            self.len -= map.len();
            self.bytes -= map.values().map(|e| e.size).sum::<usize>();
        }
    }
}

/// An estimate of the memory that `v` holds: its text, and [`ELEMENT`] for
/// each value in it.
fn size(v: &Dynamic, depth: u32) -> usize {
    if depth > MAX_DEPTH {
        return ELEMENT;
    }
    if let Some(s) = v.read_lock::<rhai::ImmutableString>() {
        return ELEMENT + s.len();
    }
    if let Some(a) = v.read_lock::<rhai::Array>() {
        return ELEMENT + a.iter().map(|x| size(x, depth + 1)).sum::<usize>();
    }
    if let Some(m) = v.read_lock::<rhai::Map>() {
        return ELEMENT + m.iter().map(|(k, x)| k.len() + size(x, depth + 1)).sum::<usize>();
    }
    if let Some(b) = v.read_lock::<rhai::Blob>() {
        return ELEMENT + b.len();
    }
    ELEMENT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn unchanged(_: &[u8]) -> Option<Stamp> {
        None
    }

    fn int(v: Option<Dynamic>) -> Option<i64> {
        v.map(|v| v.as_int().unwrap())
    }

    #[test]
    fn expires() {
        let t = Instant::now();
        let mut c = ValueCache::default();
        c.insert(1, "a".into(), 10, Vec::new(), Dynamic::from(1_i64), t);
        assert_eq!(int(c.get(1, "a", &[], t + secs(9), unchanged)), Some(1));
        assert_eq!(int(c.get(1, "a", &[], t + secs(10), unchanged)), None);
        // Another key or plugin is another entry.
        assert_eq!(int(c.get(1, "b", &[], t, unchanged)), None);
        assert_eq!(int(c.get(2, "a", &[], t, unchanged)), None);
        // 0 isn't kept.
        c.insert(1, "z".into(), 0, Vec::new(), Dynamic::from(1_i64), t);
        assert_eq!(int(c.get(1, "z", &[], t, unchanged)), None);
        // A very long time is kept.
        c.insert(1, "long".into(), u64::MAX, Vec::new(), Dynamic::from(2_i64), t);
        assert_eq!(int(c.get(1, "long", &[], t + secs(1 << 30), unchanged)), Some(2));
    }

    #[test]
    fn files() {
        let t = Instant::now();
        let mut c = ValueCache::default();
        let stamp = Stamp {
            dev: 1,
            ino: 2,
            size: 3,
            mtime: (4, 5),
        };
        let st = Some(stamp);
        let f = vec![b"f".to_vec()];
        c.insert(1, "a".into(), 60, vec![(f[0].clone(), st)], Dynamic::from(1_i64), t);
        assert_eq!(int(c.get(1, "a", &f, t, |_| st)), Some(1));
        // The file changed, or is gone.
        let other = Some(Stamp { size: 4, ..stamp });
        assert_eq!(int(c.get(1, "a", &f, t, |_| other)), None);
        assert_eq!(int(c.get(1, "a", &f, t, unchanged)), None);
        // Other files.
        assert_eq!(int(c.get(1, "a", &[], t, |_| st)), None);
        assert_eq!(int(c.get(1, "a", &[b"g".to_vec()], t, |_| st)), None);
        // A file that was missing and still is.
        c.insert(1, "m".into(), 60, vec![(f[0].clone(), None)], Dynamic::from(2_i64), t);
        assert_eq!(int(c.get(1, "m", &f, t, unchanged)), Some(2));
        assert_eq!(int(c.get(1, "m", &f, t, |_| st)), None);
    }

    #[test]
    fn misses() {
        let t = Instant::now();
        let mut c = ValueCache::default();
        let stamp = Stamp {
            dev: 1,
            ino: 2,
            size: 3,
            mtime: (4, 5),
        };
        let before = Some(stamp);
        let after = Some(Stamp { size: 9, ..stamp });
        let f = vec![b"f".to_vec()];
        // The file changed between the miss and the put: the value is
        // stale at once.
        c.missed(1, "a", f.clone(), |_| before);
        let files = c.stamps(1, "a", f.clone(), |_| after);
        assert_eq!(files, vec![(f[0].clone(), before)]);
        c.insert(1, "a".into(), 60, files, Dynamic::from(1_i64), t);
        assert_eq!(int(c.get(1, "a", &f, t, |_| after)), None);
        // A miss is used once, and only for the same files.
        assert_eq!(c.stamps(1, "a", f.clone(), |_| after), vec![(f[0].clone(), after)]);
        c.missed(1, "b", f.clone(), |_| before);
        assert_eq!(
            c.stamps(1, "b", vec![b"g".to_vec()], |_| after),
            vec![(b"g".to_vec(), after)]
        );
        assert!(c.misses.is_empty());
        // Nor are misses kept forever.
        for i in 0..MAX_MISSES + 1 {
            c.missed(1, &i.to_string(), f.clone(), |_| before);
        }
        assert!(c.misses.len() <= MAX_MISSES);
        c.remove_plugin(1);
        assert!(c.misses.is_empty());
    }

    #[test]
    fn bounded() {
        let t = Instant::now();
        let mut c = ValueCache::default();
        let big = || Dynamic::from("x".repeat(MAX_BYTES / 3));
        for (i, key) in ["a", "b", "c", "d"].iter().enumerate() {
            c.insert(1, key.to_string(), 60, Vec::new(), big(), t + secs(i as u64));
        }
        assert!(c.bytes <= MAX_BYTES);
        assert!(c.get(1, "a", &[], t + secs(5), unchanged).is_none());
        assert!(c.get(1, "d", &[], t + secs(5), unchanged).is_some());
        // Expired entries go when another is added.
        c.insert(2, "e".into(), 60, Vec::new(), Dynamic::UNIT, t + secs(100));
        assert_eq!(c.len, 1);
        assert_eq!(c.bytes, 1 + ELEMENT);
        // A value too big for the cache isn't kept.
        c.insert(
            2,
            "f".into(),
            60,
            Vec::new(),
            Dynamic::from("x".repeat(MAX_BYTES)),
            t + secs(100),
        );
        assert!(c.get(2, "f", &[], t + secs(100), unchanged).is_none());
        // Nor what an unloaded plugin kept.
        c.remove_plugin(2);
        assert_eq!((c.len, c.bytes), (0, 0));
    }

    #[test]
    fn sizes() {
        let a: rhai::Array = vec!["ab".into(), Dynamic::from(1_i64)];
        let mut m = rhai::Map::new();
        m.insert("key".into(), a.into());
        assert_eq!(size(&m.into(), 0), ELEMENT + 3 + ELEMENT + (ELEMENT + 2) + ELEMENT);
    }
}
