//! Cached startup files (the first part of the Stage 3 design in PLAN.md).
//!
//! Two directories in `$XDG_CONFIG_HOME/luish/` hold `*.lsh` files whose
//! effects are cached, as zsh's `.zshrc` and `.zlogin`: `rc.d/`, for every
//! interactive shell, and `login.d/`, for login shells, after `rc.d` (and
//! instead of `/etc/profile` and `~/.profile`). Each directory's files run
//! in byte order, and its cache is `$XDG_CACHE_HOME/luish/NAME-HOST`
//! (`rc-HOST`, `login-HOST`).
//!
//! The cache holds entries, each what one file or one `__luish_cache` block
//! changed (variables, functions, aliases, options, traps, `umask` and the
//! directory; see `state.rs`) for one key, with the files it read with `.`
//! and their fingerprints. A few keys are kept for each file or block
//! ([`KEEP`]), so that shells started in different environments don't
//! evict each other's entries.
//!
//! - A file without a block (outside functions, [`has_block`]) is one
//!   entry, keyed on its fingerprint, `PATH` and `HOME`, and the chain: a
//!   hash of the ids, keys and changes of every entry before it in its
//!   directory, so that a file runs again when one before it does something
//!   else.
//! - A file with a block runs every time, apart from its blocks: the cache
//!   only records that it has blocks (a "mixed" entry, by fingerprint).
//! - A block is keyed on the values of the variables of its `env=(...)` and
//!   the fingerprints of the paths of its `files=(...)`; its id has the hash
//!   of its text (unparsed, so comments don't count) and its file. A block
//!   inside another, or inside a cached file, runs as part of it.
//! - `rc.d`'s first entry is `config.toml` (see `config.rs`) and the plugins
//!   it enables (`plugins/package.rs`), with `plugins.lock` and the
//!   manifests of local plugins among its files; its last is the plugins'
//!   `post-rc.lsh` files. Their `post-rc` hooks run after, in every shell
//!   (before `_uncached.lsh`). With `config.toml`, `rc.d`'s cache is used
//!   even if the directory doesn't exist. `--no-plugins` bypasses the caches.
//! - A directory's `_uncached.lsh` runs every time, after the rest; blocks
//!   in it are cached.
//!
//! `$ENV` and `luishrc` (`interactive::startup`) aren't in a directory and
//! always run (as `_uncached.lsh` does), but their blocks, and those of the
//! plugins they load with `plugin load`, are cached, in a cache of their
//! own, `startup-HOST` ([`begin_startup`], [`finish_startup`]): they have no
//! file-level entries, chain or `_uncached.lsh`.
//!
//! The cache also records the build of luish that wrote it (another build
//! discards it), the directory, and, for `__luish_internal check-cache`, when
//! it was last built and the environment and options of the shell that
//! built it. Entries that a startup doesn't use are dropped if they are for
//! a file that no longer exists, or for a block built more than [`MAX_AGE`]
//! ago.
//!
//! `__luish_internal check-cache` ([`check`]) looks for the changes that the
//! keys miss: it starts a shell as the last one that built the cache (its
//! options and environment), which rebuilds every entry it reaches
//! ([`Run::check`]), and compares them with the cache's. If they are the
//! same, the cache is only touched (its modification time is when it was
//! last found current); otherwise the new entries replace the old.

use std::ffi::CString;
use std::hash::Hasher;

use crate::ast::{CacheBlock, Command, CompoundCommand, List};
use crate::builtins::internal::BUILD_ID;
use crate::hash::FastHasher;
use crate::interactive::{run_file, source_file, to_path};
use crate::shell::{ExecResult, Shell};
use crate::state::{self, Change, Kind};
use crate::sys;

/// Bumped when the format of the cache changes.
const HEADER: &[u8] = b"# luish startup cache 5\n";

/// How many keys are kept for each file or block.
const KEEP: usize = 4;

/// How long an entry for a block that no startup uses is kept, in seconds.
const MAX_AGE: i64 = 30 * 86400;

/// The variables that the entries of files are keyed on.
const DEFAULT_ENV: &[&[u8]] = &[b"PATH", b"HOME"];

/// What a file looked like: device, inode, size, modification time.
type Stamp = Option<(u64, u64, i64, i64, i64)>;

fn stamp(path: &[u8]) -> Stamp {
    sys::stat(path).map(|st| (st.st_dev, st.st_ino, st.st_size, st.st_mtime, st.st_mtime_nsec))
}

fn format_stamp(s: &Stamp) -> String {
    match s {
        Some((dev, ino, size, sec, nsec)) => format!("{dev} {ino} {size} {sec} {nsec}"),
        None => "- - - - -".to_string(),
    }
}

/// A file that an entry read with `.`, and its fingerprint then.
#[derive(Debug, Clone, PartialEq)]
struct Dep {
    path: Vec<u8>,
    stamp: String,
}

impl Dep {
    fn current(&self) -> bool {
        format_stamp(&stamp(&self.path)) == self.stamp
    }
}

/// What a file or a block changed, for one key.
#[derive(Debug, Clone)]
struct Entry {
    /// `config`, `post-rc`, `file NAME` or `block HASH PATH`.
    id: Vec<u8>,
    /// Items made by [`item`].
    key: Vec<u8>,
    /// When it was built, in seconds since the epoch.
    time: i64,
    /// The line of a block.
    line: u32,
    deps: Vec<Dep>,
    /// The exit status of a block.
    status: i32,
    /// What it changed, as [`encode_changes`] writes it.
    changes: Vec<u8>,
    /// Used or built by this shell (not saved).
    seen: bool,
}

/// A cache file: the build of luish that wrote it, the directory it was
/// built from, when and in what environment it was last built, and the
/// entries.
struct Cache {
    build: Vec<u8>,
    dir: Vec<u8>,
    /// When an entry was last built, in seconds since the epoch (UTC).
    time: i64,
    /// The options that start a shell like the one that last built an
    /// entry: `-i`, `-l` or `-il`.
    mode: Vec<u8>,
    /// The environment that shell started with: `NAME=VALUE` strings, each
    /// followed by a NUL.
    env: Vec<u8>,
    entries: Vec<Entry>,
}

/// Appends a field of the cache file: `TAG LEN`, a newline, the bytes and a
/// newline.
fn field(out: &mut Vec<u8>, tag: u8, value: &[u8]) {
    out.push(tag);
    out.extend_from_slice(format!(" {}\n", value.len()).as_bytes());
    out.extend_from_slice(value);
    out.push(b'\n');
}

/// Takes a field that [`field`] wrote from the start of `rest`.
fn take_field<'a>(rest: &mut &'a [u8]) -> Option<(u8, &'a [u8])> {
    let (&tag, after) = rest.split_first()?;
    let after = after.strip_prefix(b" ")?;
    let nl = after.iter().position(|&c| c == b'\n')?;
    let len: usize = std::str::from_utf8(&after[..nl]).ok()?.parse().ok()?;
    let value = after.get(nl + 1..nl + 1 + len)?;
    *rest = after[nl + 1 + len..].strip_prefix(b"\n")?;
    Some((tag, value))
}

fn number<T: std::str::FromStr>(b: &[u8]) -> Option<T> {
    std::str::from_utf8(b).ok()?.parse().ok()
}

fn encode_changes(changes: &[Change]) -> Vec<u8> {
    let mut out = Vec::new();
    for c in changes {
        put(&mut out, &[c.kind.code(), b'0' + c.removed as u8]);
        put(&mut out, &c.name);
        put(&mut out, &c.text);
    }
    out
}

/// The commands that make the changes that [`encode_changes`] wrote:
/// removals first (see `state::changes`).
fn render(mut b: &[u8]) -> Vec<u8> {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    while let (Some(&[kind, r]), Some(_), Some(text)) = (take(&mut b), take(&mut b), take(&mut b)) {
        let Some(kind) = Kind::from_code(kind) else {
            break;
        };
        if r == b'1' {
            removed.extend_from_slice(text);
        } else {
            kept.push((kind, text));
        }
    }
    state::join(&mut removed, &kept);
    removed
}

fn decode_changes(mut b: &[u8]) -> Option<Vec<Change>> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let &[kind, removed] = take(&mut b)? else {
            return None;
        };
        out.push(Change {
            kind: Kind::from_code(kind)?,
            removed: removed == b'1',
            name: take(&mut b)?.to_vec(),
            text: take(&mut b)?.to_vec(),
        });
    }
    Some(out)
}

impl Cache {
    fn new(dir: &[u8]) -> Cache {
        Cache {
            build: BUILD_ID.as_bytes().to_vec(),
            dir: dir.to_vec(),
            time: 0,
            mode: Vec::new(),
            env: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn parse(text: &[u8]) -> Option<Cache> {
        let mut rest = text.strip_prefix(HEADER)?;
        let mut cache = Cache::new(b"");
        cache.build.clear();
        while !rest.is_empty() {
            let (tag, value) = take_field(&mut rest)?;
            let entry = cache.entries.last_mut();
            match (tag, entry) {
                (b'b', _) => cache.build = value.to_vec(),
                (b'd', _) => cache.dir = value.to_vec(),
                (b't', _) => cache.time = number(value)?,
                (b'm', _) => cache.mode = value.to_vec(),
                (b'e', _) => {
                    cache.env.extend_from_slice(value);
                    cache.env.push(0);
                }
                (b'E', _) => cache.entries.push(Entry {
                    id: value.to_vec(),
                    key: Vec::new(),
                    time: 0,
                    line: 0,
                    deps: Vec::new(),
                    status: 0,
                    changes: Vec::new(),
                    seen: false,
                }),
                (b'K', Some(e)) => e.key = value.to_vec(),
                (b'T', Some(e)) => e.time = number(value)?,
                (b'L', Some(e)) => e.line = number(value)?,
                (b'R', Some(e)) => e.status = number(value)?,
                (b'S', Some(e)) => {
                    // Five fields of the fingerprint, then the path.
                    let mut parts = value.splitn(6, |&c| c == b' ');
                    let stamp: Vec<&[u8]> = parts.by_ref().take(5).collect();
                    e.deps.push(Dep {
                        stamp: String::from_utf8(stamp.join(&b' ')).ok()?,
                        path: parts.next()?.to_vec(),
                    });
                }
                (b'C', Some(e)) => e.changes = value.to_vec(),
                _ => return None,
            }
        }
        Some(cache)
    }

    fn serialize(&self) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        field(&mut out, b'b', &self.build);
        field(&mut out, b'd', &self.dir);
        field(&mut out, b't', self.time.to_string().as_bytes());
        field(&mut out, b'm', &self.mode);
        for var in self.env.split(|&c| c == 0).filter(|v| !v.is_empty()) {
            field(&mut out, b'e', var);
        }
        for e in &self.entries {
            field(&mut out, b'E', &e.id);
            field(&mut out, b'K', &e.key);
            field(&mut out, b'T', e.time.to_string().as_bytes());
            if e.line != 0 {
                field(&mut out, b'L', e.line.to_string().as_bytes());
            }
            if e.status != 0 {
                field(&mut out, b'R', e.status.to_string().as_bytes());
            }
            for d in &e.deps {
                field(&mut out, b'S', &[d.stamp.as_bytes(), b" ", &d.path].concat());
            }
            field(&mut out, b'C', &e.changes);
        }
        out
    }

    /// Adds `new`, replacing the entry with its id and key. For a file, the
    /// entries for other versions of it go; otherwise the oldest entries
    /// beyond [`KEEP`] for the id do.
    fn store(&mut self, new: Entry) {
        let new_stamp = file_stamp(&new).map(<[u8]>::to_vec);
        let is_file = new.id.starts_with(b"file ");
        self.entries
            .retain(|e| e.id != new.id || (e.key != new.key && !(is_file && file_stamp(e) != new_stamp.as_deref())));
        let mut same: Vec<(i64, usize)> = (self.entries.iter().enumerate())
            .filter(|(_, e)| e.id == new.id)
            .map(|(i, e)| (e.time, i))
            .collect();
        if same.len() >= KEEP {
            same.sort();
            let old: Vec<usize> = same[..same.len() + 1 - KEEP].iter().map(|&(_, i)| i).collect();
            let mut i = 0;
            self.entries.retain(|_| {
                i += 1;
                !old.contains(&(i - 1))
            });
        }
        self.entries.push(new);
    }

    /// Records that this shell built the cache `name`: the time, and how to
    /// start a shell like it (for `check-cache`).
    fn built_by(&mut self, sh: &Shell, name: &[u8]) {
        self.time = sys::now();
        let mode: &[u8] = match (name, sh.interactive) {
            (b"login", true) => b"-il",
            (b"login", false) => b"-l",
            _ => b"-i",
        };
        self.mode = mode.to_vec();
        self.env = environment();
    }

    /// The entry with this id and key whose files are unchanged.
    fn find(&self, id: &[u8], key: &[u8]) -> Option<usize> {
        (self.entries.iter()).position(|e| e.id == id && e.key == key && e.deps.iter().all(Dep::current))
    }
}

/// The fingerprint of the file of a file's entry (whole or mixed).
fn file_stamp(e: &Entry) -> Option<&[u8]> {
    items(&e.key).find(|i| matches!(i.0, b'f' | b'm')).map(|i| i.1)
}

/// Appends an item of a key: its tag, then the value with its length.
fn item(key: &mut Vec<u8>, tag: u8, value: &[u8]) {
    key.push(tag);
    put(key, value);
}

/// The items of a key.
fn items(mut key: &[u8]) -> impl Iterator<Item = (u8, &[u8])> {
    std::iter::from_fn(move || {
        let (&tag, rest) = key.split_first()?;
        key = rest;
        Some((tag, take(&mut key)?))
    })
}

/// The values of variables, for a key: `v NAME=VALUE`, or `u NAME` if unset.
fn env_items(sh: &Shell, key: &mut Vec<u8>, names: &[&[u8]]) {
    for name in names {
        match sh.get_var(name) {
            Some(v) => item(key, b'v', &[name, &b"="[..], &v].concat()),
            None => item(key, b'u', name),
        }
    }
}

/// A startup cache being used: by [`run`], and by [`run_block`] while a
/// file with blocks runs.
pub struct Run {
    cache: Cache,
    /// Where the cache is written (none without `HOME`, or with
    /// `--no-plugins`).
    path: Option<Vec<u8>>,
    /// Whether an entry was added or removed.
    changed: bool,
    /// The ids and keys of the entries so far.
    chain: FastHasher,
    /// The startup file that is running, for the ids of its blocks.
    file: Vec<u8>,
    /// In the shell that `check-cache` starts: rebuild every entry.
    check: bool,
    /// A plugin that `config.toml` enables isn't installed: the entry for
    /// `config.toml` isn't saved.
    incomplete: bool,
}

impl Run {
    fn new(cache: Cache, path: Option<Vec<u8>>, check: bool) -> Run {
        Run {
            cache,
            path,
            changed: false,
            chain: FastHasher::default(),
            file: Vec::new(),
            check,
            incomplete: false,
        }
    }

    fn chain_value(&self) -> String {
        format!("{:016x}", self.chain.finish())
    }

    /// Adds an entry to the chain: its id, its key and what it changed.
    fn link(&mut self, id: &[u8], key: &[u8], changes: &[u8]) {
        link(&mut self.chain, id, key, changes);
    }

    /// The entry to use for this id and key, if any (none when checking).
    fn lookup(&mut self, id: &[u8], key: &[u8]) -> Option<usize> {
        let i = self.cache.find(id, key).filter(|_| !self.check)?;
        self.cache.entries[i].seen = true;
        Some(i)
    }

    fn store(&mut self, mut e: Entry) {
        e.seen = true;
        self.cache.store(e);
        self.changed = true;
    }
}

fn link(chain: &mut FastHasher, id: &[u8], key: &[u8], changes: &[u8]) {
    for b in [id, key, changes] {
        chain.write_usize(b.len());
        chain.write(b);
    }
}

/// Uses the entry `i`: makes its changes, and adds it to the chain. The
/// commands are read without aliases: those that the text has were expanded
/// when it was written, and those of earlier entries mustn't be.
fn replay(sh: &mut Shell, run: &mut Run, i: usize) {
    let e = &run.cache.entries[i];
    link(&mut run.chain, &e.id, &e.key, &e.changes);
    crate::interactive::run_text(sh, &render(&e.changes), false);
}

/// `$XDG_CONFIG_HOME` (or `~/.config`), or `$XDG_CACHE_HOME` (or `~/.cache`).
pub fn xdg_dir(sh: &Shell, var: &[u8], default: &[u8]) -> Option<Vec<u8>> {
    sh.get_var(var).filter(|c| c.first() == Some(&b'/')).or_else(|| {
        sh.get_var(b"HOME").filter(|h| !h.is_empty()).map(|mut h| {
            h.extend_from_slice(default);
            h
        })
    })
}

/// The directory `luish/NAME` in the configuration directory, if it exists.
pub fn config_dir(sh: &Shell, name: &[u8]) -> Option<Vec<u8>> {
    let mut d = xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    d.extend_from_slice(b"/luish/");
    d.extend_from_slice(name);
    sys::is_dir(&d).then_some(d)
}

fn hostname() -> Vec<u8> {
    let name: Vec<u8> = sys::hostname()
        .into_iter()
        .map(|c| if c == b'/' { b'_' } else { c })
        .collect();
    if name.is_empty() { b"localhost".to_vec() } else { name }
}

/// The files in `dir` that are cached, in byte order, with their stamps.
fn cached_files(dir: &[u8]) -> Vec<(Vec<u8>, String)> {
    let mut names: Vec<Vec<u8>> = sys::read_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n.ends_with(b".lsh") && n.first() != Some(&b'.') && n != b"_uncached.lsh")
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let st = sys::stat(&join(dir, &name))?;
            let s = Some((st.st_dev, st.st_ino, st.st_size, st.st_mtime, st.st_mtime_nsec));
            (st.st_mode & libc::S_IFMT == libc::S_IFREG).then(|| (name, format_stamp(&s)))
        })
        .collect()
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    [dir, b"/", name].concat()
}

/// `$XDG_CACHE_HOME/luish` (or `~/.cache/luish`).
pub fn cache_dir(sh: &Shell) -> Option<Vec<u8>> {
    let mut c = xdg_dir(sh, b"XDG_CACHE_HOME", b"/.cache")?;
    c.extend_from_slice(b"/luish");
    Some(c)
}

/// The cache `luish/NAME-HOST` in the cache directory.
fn cache_file(sh: &Shell, name: &[u8]) -> Option<Vec<u8>> {
    let mut c = cache_dir(sh)?;
    c.push(b'/');
    c.extend_from_slice(name);
    c.push(b'-');
    c.extend(hostname());
    Some(c)
}

/// Whether the text has a `__luish_cache` block outside a function.
fn has_block(sh: &Shell, text: &[u8]) -> bool {
    fn list(l: &List) -> bool {
        l.iter().any(|cc| {
            let ao = &cc.list;
            std::iter::once(&ao.first)
                .chain(ao.rest.iter().map(|(_, p)| p))
                .any(|p| p.cmds.iter().any(command))
        })
    }
    fn command(c: &Command) -> bool {
        match c {
            Command::Cache(_) => true,
            Command::Simple(_) | Command::FunctionDef { .. } => false,
            Command::Compound(cc, _) => match cc {
                CompoundCommand::BraceGroup(l) | CompoundCommand::Subshell(l) => list(l),
                CompoundCommand::If { conds, else_ } => {
                    conds.iter().any(|(c, b)| list(c) || list(b)) || else_.as_ref().is_some_and(list)
                }
                CompoundCommand::While { cond, body, .. } => list(cond) || list(body),
                CompoundCommand::For { body, .. } => list(body),
                CompoundCommand::Case { arms, .. } => arms.iter().any(|a| list(&a.body)),
                CompoundCommand::Cond { .. } => false,
            },
        }
    }
    if !text.windows(13).any(|w| w == b"__luish_cache") {
        return false;
    }
    let mut p = crate::lexer::Parser::new(text.to_vec(), 1, true);
    p.bareglobqual = sh.opt(crate::options::Opt::Bareglobqual);
    let aliases = sh.aliases.clone();
    while let Ok(Some(l)) = p.parse_next(&aliases) {
        if list(&l) {
            return true;
        }
    }
    false
}

/// Runs `f` and returns what it changed in the state, and the files it read
/// with `.`.
fn build<R>(sh: &mut Shell, f: impl FnOnce(&mut Shell) -> R) -> (R, Vec<Dep>, Vec<u8>) {
    sh.vars.clear_assigned();
    let before = sh.state_entries();
    let outer = sh.sourced_files.replace(Vec::new());
    let r = f(sh);
    let sourced = std::mem::replace(&mut sh.sourced_files, outer).unwrap_or_default();
    let mut deps: Vec<Dep> = Vec::new();
    for path in sourced {
        if !deps.iter().any(|d| d.path == path) {
            deps.push(Dep {
                stamp: format_stamp(&stamp(&path)),
                path,
            });
        }
    }
    let changes = encode_changes(&state::changes(&before, &sh.state_entries()));
    (r, deps, changes)
}

fn new_entry(id: Vec<u8>, key: Vec<u8>, deps: Vec<Dep>, changes: Vec<u8>) -> Entry {
    Entry {
        id,
        key,
        time: sys::now(),
        line: 0,
        deps,
        status: 0,
        changes,
        seen: true,
    }
}

/// Runs what the entry `id` caches (`f`, which returns whether it can be
/// saved), from the cache if it has the key of the default variables, the
/// chain and `stamp` (a file's).
fn cached(sh: &mut Shell, run: &mut Run, id: &[u8], stamp: Option<&str>, f: impl FnOnce(&mut Shell) -> bool) {
    let mut key = Vec::new();
    if let Some(s) = stamp {
        item(&mut key, b'f', s.as_bytes());
    }
    env_items(sh, &mut key, DEFAULT_ENV);
    item(&mut key, b'c', run.chain_value().as_bytes());
    match run.lookup(id, &key) {
        Some(i) => replay(sh, run, i),
        None => {
            let (complete, deps, changes) = build(sh, f);
            run.link(id, &key, &changes);
            if complete {
                run.store(new_entry(id.to_vec(), key, deps, changes));
            }
        }
    }
}

/// Runs the startup files in `dir`, using the cache `luish/NAME-HOST`.
/// `config` is `config.toml`, applied first and cached with the files
/// (whether it exists or not, so that creating it is noticed).
pub fn run(sh: &mut Shell, dir: &[u8], name: &[u8], config: Option<&[u8]>) {
    let files = cached_files(dir);
    // A shell without plugins would save a state without them.
    let path = cache_file(sh, name).filter(|_| !sh.no_plugins);
    let check = sh.check_cache.as_ref().is_some_and(|c| c.0 == name);
    let old = (path.as_ref())
        .and_then(|p| std::fs::read(to_path(p)).ok())
        .and_then(|t| Cache::parse(&t))
        .filter(|c| c.build == BUILD_ID.as_bytes() && c.dir == dir);
    let mut run = Run::new(old.unwrap_or_else(|| Cache::new(dir)), path, check);
    let rc = config.is_some();
    sh.in_rc = rc;
    if let Some(c) = config {
        let mut incomplete = false;
        cached(sh, &mut run, b"config", None, |sh| {
            crate::config::load(sh, c);
            if let Some(rec) = &mut sh.sourced_files {
                rec.push(c.to_vec());
            }
            // Plugins that aren't installed are reported by every shell
            // until they are.
            incomplete = !crate::plugins::load_enabled(sh);
            !incomplete
        });
        run.incomplete = incomplete;
    }
    for (file, fstamp) in &files {
        let path = join(dir, file);
        run.file = path.clone();
        let id = [b"file ", &file[..]].concat();
        let mut mixed = Vec::new();
        item(&mut mixed, b'm', fstamp.as_bytes());
        // Whether a file has blocks only depends on its text: not rebuilt
        // when checking.
        if let Some(i) = run.cache.find(&id, &mixed) {
            run.cache.entries[i].seen = true;
            run_mixed(sh, &mut run, &path, None);
            run.link(&id, &mixed, &[]);
            continue;
        }
        // A file that is new or changed is read once, to see whether it has
        // blocks.
        let known = (run.cache.entries.iter()).any(|e| e.id == id && file_stamp(e) == Some(fstamp.as_bytes()));
        let text = (!known).then(|| std::fs::read(to_path(&path)).ok()).flatten();
        if !text.as_ref().is_some_and(|t| has_block(sh, t)) {
            cached(sh, &mut run, &id, Some(fstamp), |sh| {
                match text {
                    Some(t) => run_file(sh, &path, &t),
                    None => source_file(sh, &path),
                }
                true
            });
        } else {
            run.store(new_entry(id.clone(), mixed.clone(), Vec::new(), Vec::new()));
            run_mixed(sh, &mut run, &path, text);
            run.link(&id, &mixed, &[]);
        }
    }
    if rc {
        cached(sh, &mut run, b"post-rc", None, |sh| {
            crate::plugins::post_rc_files(sh);
            true
        });
    }
    sh.in_rc = false;
    if check {
        check_child(sh, run);
    }
    // Saved before `_uncached.lsh`, in case it exits.
    save(sh, &mut run, name);
    // Extensions run in every shell, from the cache too.
    if rc {
        crate::plugins::post_rc_hooks(sh);
    }
    let uncached = join(dir, b"_uncached.lsh");
    if sys::stat(&uncached).is_some() {
        run.file = uncached.clone();
        run_mixed(sh, &mut run, &uncached, None);
    }
    // Entries for files that are gone, or for blocks no shell has used in a
    // while.
    let now = sys::now();
    let before = run.cache.entries.len();
    run.cache.entries.retain(|e| {
        e.seen
            || match e.id.strip_prefix(b"file ") {
                Some(f) => files.iter().any(|(name, _)| name == f),
                None => !e.id.starts_with(b"block ") || now - e.time < MAX_AGE,
            }
    });
    run.changed |= run.cache.entries.len() != before;
    save(sh, &mut run, name);
}

/// Runs a file with blocks, which use the cache, or its `text` if read.
/// Also used for `$ENV` and `luishrc` ([`begin_startup`]), whose own text
/// always runs, like a mixed file's, with only their blocks cached.
pub(crate) fn run_mixed(sh: &mut Shell, run: &mut Run, path: &[u8], text: Option<Vec<u8>>) {
    let placeholder = Run::new(Cache::new(b""), None, false);
    sh.startcache = Some(Box::new(std::mem::replace(run, placeholder)));
    match text {
        Some(t) => run_file(sh, path, &t),
        None => source_file(sh, path),
    }
    if let Some(r) = sh.startcache.take() {
        *run = *r;
    }
}

/// Starts the cache for the blocks of `$ENV`, `luishrc` and the plugins
/// loaded while they run (`interactive::startup`, with [`run_mixed`] for
/// each file and [`finish_startup`] at the end): `luish/startup-HOST`, of
/// its own since neither file is cached as a whole.
pub fn begin_startup(sh: &Shell) -> Run {
    let path = cache_file(sh, b"startup").filter(|_| !sh.no_plugins);
    let check = sh.check_cache.as_ref().is_some_and(|c| c.0 == b"startup");
    let old = (path.as_ref())
        .and_then(|p| std::fs::read(to_path(p)).ok())
        .and_then(|t| Cache::parse(&t))
        .filter(|c| c.build == BUILD_ID.as_bytes());
    Run::new(old.unwrap_or_else(|| Cache::new(b"")), path, check)
}

/// Ends the cache that [`begin_startup`] started: in the shell that
/// `__luish_internal check-cache startup` starts, reports it (and exits);
/// otherwise drops the block entries no shell has used in a while, and
/// saves the cache if it changed.
pub fn finish_startup(sh: &mut Shell, mut run: Run) {
    if run.check {
        check_child(sh, run); // exits
    }
    let now = sys::now();
    let before = run.cache.entries.len();
    run.cache.entries.retain(|e| e.seen || now - e.time < MAX_AGE);
    run.changed |= run.cache.entries.len() != before;
    save(sh, &mut run, b"startup");
}

/// Writes the cache if it changed, with the environment and options of
/// this shell.
fn save(sh: &Shell, run: &mut Run, name: &[u8]) {
    let Some(path) = run.path.as_ref().filter(|_| run.changed) else {
        return;
    };
    run.cache.built_by(sh, name);
    if let Err(e) = write_cache(path, &run.cache.serialize()) {
        sh.error(format!(
            "cannot write startup cache {}: {}",
            String::from_utf8_lossy(path),
            sys::strerror(e)
        ));
    }
    run.changed = false;
}

/// Runs a `__luish_cache` block while a startup file runs: from the cache if
/// it has the block's key, otherwise running it and saving what it changed.
pub fn run_block(sh: &mut Shell, block: &CacheBlock) -> ExecResult {
    let Some(mut run) = sh.startcache.take() else {
        return sh.run_list(&block.body);
    };
    let r = block_in(sh, &mut run, block);
    sh.startcache = Some(run);
    r
}

fn block_in(sh: &mut Shell, run: &mut Run, block: &CacheBlock) -> ExecResult {
    let mut h = FastHasher::default();
    let text = crate::unparse::cache_block(block);
    h.write_usize(text.len());
    h.write(&text);
    let id = [format!("block {:016x} ", h.finish()).as_bytes(), &run.file].concat();
    let mut key = Vec::new();
    let names: Vec<&[u8]> = block.env.iter().map(|n| &n[..]).collect();
    env_items(sh, &mut key, &names);
    for p in sh.expand_words(&block.files)? {
        item(
            &mut key,
            b'p',
            &[format_stamp(&stamp(&p)).as_bytes(), b" ", &p].concat(),
        );
    }
    if let Some(i) = run.lookup(&id, &key) {
        let status = run.cache.entries[i].status;
        replay(sh, run, i);
        return Ok(status);
    }
    let (r, deps, changes) = build(sh, |sh| sh.run_list(&block.body));
    // `return`, `break` or an error: not saved.
    let status = r?;
    run.link(&id, &key, &changes);
    let mut e = new_entry(id, key, deps, changes);
    e.line = block.lineno;
    e.status = status;
    run.store(e);
    Ok(status)
}

/// The environment the shell started with (the shell never changes its
/// own), as `NAME=VALUE` strings each followed by a NUL.
fn environment() -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let mut out = Vec::new();
    for (k, v) in std::env::vars_os() {
        out.extend_from_slice(k.as_bytes());
        out.push(b'=');
        out.extend_from_slice(v.as_bytes());
        out.push(0);
    }
    out
}

/// The cache directory tag (https://bford.info/cachedir/), so that backup
/// tools skip the directory.
const CACHEDIR_TAG: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55
# This file is a cache directory tag created by luish.
# For information about cache directory tags, see https://bford.info/cachedir/
";

const README: &[u8] = b"This directory holds caches written by luish, the shell:

- rc-HOST, login-HOST: the effects of the startup files in
  ~/.config/luish/rc.d/ and ~/.config/luish/login.d/ (variables,
  functions, aliases, options, ...) and of their __luish_cache blocks,
  saved on the host HOST, so that new shells restore them instead of
  running the files. Each also holds the environment of the shell that
  last built it, so that `__luish_internal check-cache` can run the files
  again as it did.
- plugins/git/: git repositories of plugins, which `plugin sync` and
  `plugin update` fetch into (the files that shells load are in
  ~/.local/share/luish/plugins/).

Everything here can be recreated: the directory can be removed at any time
without losing anything (the next shell rebuilds what it needs, and
`plugin sync` fetches again).
";

/// Adds `CACHEDIR.TAG` and `README` to the cache directory, if missing.
/// Failures are ignored: the cache works without them.
pub fn mark_cache_dir(dir: &std::path::Path) {
    for (name, text) in [("CACHEDIR.TAG", CACHEDIR_TAG), ("README", README)] {
        let path = dir.join(name);
        if !path.exists() {
            let _ = std::fs::write(path, text);
        }
    }
}

/// Writes the cache with mode 0600 (exported variables can hold tokens),
/// through a temporary file renamed into place.
fn write_cache(path: &[u8], text: &[u8]) -> Result<(), i32> {
    use std::os::unix::fs::DirBuilderExt;
    let slash = path.iter().rposition(|&c| c == b'/').unwrap_or(0);
    let parent = to_path(&path[..slash]);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&parent)
        .map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))?;
    mark_cache_dir(&parent);
    let (fd, tmp) = sys::mkstemp(&[path, b"."].concat())?;
    let ok = sys::write_all(fd, text);
    let e = sys::errno();
    sys::close(fd);
    let r = if !ok {
        Err(e)
    } else {
        std::fs::rename(to_path(&tmp), to_path(path)).map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))
    };
    if r.is_err() {
        sys::unlink(&tmp);
    }
    r
}

// ----------------------------------------------------------------------
// `__luish_internal check-cache`

/// Appends `b` with its length, on a line of its own before it.
fn put(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(format!("{}\n", b.len()).as_bytes());
    out.extend_from_slice(b);
}

/// Takes what [`put`] appended from the start of `rest`.
fn take<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let nl = rest.iter().position(|&c| c == b'\n')?;
    let len: usize = std::str::from_utf8(&rest[..nl]).ok()?.parse().ok()?;
    let b = rest.get(nl + 1..nl + 1 + len)?;
    *rest = &rest[nl + 1 + len..];
    Some(b)
}

/// In the shell that [`check`] starts, once it has rebuilt every entry of
/// the cache it checks: writes them to the file that `check` named, or why
/// they can't be saved, and exits.
fn check_child(sh: &mut Shell, run: Run) -> ! {
    let (name, out) = sh.check_cache.take().unwrap_or_default();
    let mut report = Vec::new();
    if run.incomplete {
        put(&mut report, b"a plugin that config.toml enables isn't installed");
    } else {
        let mut cache = run.cache;
        cache.entries.retain(|e| e.seen);
        cache.built_by(sh, &name);
        put(&mut report, b"ok");
        put(&mut report, &cache.serialize());
    }
    let _ = std::fs::write(to_path(&out), report);
    sys::exit(0)
}

/// In the shell that [`check`] starts, when it didn't reach the cache to
/// check (its directory is gone).
pub fn check_not_reached(sh: &mut Shell) -> ! {
    let (_, out) = sh.check_cache.take().unwrap_or_default();
    let mut report = Vec::new();
    put(&mut report, b"its startup directory no longer exists");
    let _ = std::fs::write(to_path(&out), report);
    sys::exit(0)
}

const CHECK: &[u8] = b"__luish_internal check-cache";

/// `__luish_internal check-cache [-q] [rc|login|startup]...`: checks the
/// startup caches (those of this host that exist, by default) by running
/// their files again. Status 0 if they were current (they are then
/// touched), 1 if any was replaced, 2 on errors. The report is printed only
/// for the caches that were replaced, with `-q`.
pub fn check(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut quiet = false;
    let mut names: Vec<&[u8]> = Vec::new();
    let mut options = true;
    for a in &argv[1..] {
        match a.as_slice() {
            b"-q" | b"--quiet" if options => quiet = true,
            b"--" if options => options = false,
            b"rc" | b"login" | b"startup" => {
                options = false;
                names.push(a);
            }
            _ if options && a.first() == Some(&b'-') => {
                sh.berr(CHECK, format!("illegal option {}", String::from_utf8_lossy(a)));
                return Ok(2);
            }
            _ => {
                let msg = format!("{}: no such cache (rc, login or startup)", String::from_utf8_lossy(a));
                sh.berr(CHECK, msg);
                return Ok(2);
            }
        }
    }
    if sh.no_plugins {
        sh.berr(CHECK, "startup caches are disabled (--no-plugins)");
        return Ok(2);
    }
    let named = !names.is_empty();
    if !named {
        names = vec![b"rc", b"login", b"startup"];
    }
    let mut status = 0;
    let mut found = false;
    for name in names {
        let Some(path) = cache_file(sh, name) else {
            sh.berr(CHECK, "no cache directory (HOME is not set)");
            return Ok(2);
        };
        let shown = String::from_utf8_lossy(&path).into_owned();
        let text = match std::fs::read(to_path(&path)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !named => continue,
            Err(e) => {
                let msg = format!("cannot read {shown}: {}", sys::strerror(e.raw_os_error().unwrap_or(0)));
                sh.berr(CHECK, msg);
                status = 2;
                continue;
            }
        };
        found = true;
        let Some(cache) = Cache::parse(&text) else {
            let msg = format!("{shown}: not a cache of this version of luish (the next shell replaces it)");
            sh.berr(CHECK, msg);
            status = 2;
            continue;
        };
        match check_one(sh, name, &path, cache) {
            Ok((changed, report)) => {
                if changed {
                    status = status.max(1);
                }
                if changed || !quiet {
                    sh.out(&report);
                }
            }
            Err(msg) => {
                sh.berr(CHECK, format!("{shown}: {msg}"));
                status = 2;
            }
        }
    }
    if !found && !quiet && status == 0 {
        sh.out(b"no startup caches\n");
    }
    Ok(status)
}

/// Checks one cache: whether it changed, and the report.
fn check_one(sh: &mut Shell, name: &[u8], path: &[u8], cache: Cache) -> Result<(bool, Vec<u8>), String> {
    let mtime = sys::stat(path).map(|st| st.st_mtime);
    let (fd, tmp) = sys::mkstemp(&[path, b".check."].concat())
        .map_err(|e| format!("cannot create a temporary file: {}", sys::strerror(e)))?;
    sys::close(fd);
    let result = rerun(sh, name, &cache, &tmp);
    sys::unlink(&tmp);
    let result = result?;
    let mut rest = result.as_slice();
    let bad = || "the check failed".to_string();
    let status = take(&mut rest).ok_or_else(bad)?;
    if status != b"ok" {
        return Err(format!("cannot rebuild it: {}", String::from_utf8_lossy(status)));
    }
    let new = Cache::parse(take(&mut rest).ok_or_else(bad)?).ok_or_else(bad)?;

    let mut out = format!("{}: {}\n", String::from_utf8_lossy(name), String::from_utf8_lossy(path)).into_bytes();
    let now = sys::now();
    let locale = time_locale(sh);
    let when = |t: i64| {
        let mut s = sys::format_time(t, b"%c", &locale);
        s.extend_from_slice(ago(now - t).as_bytes());
        s
    };
    out.extend_from_slice(b"  generated ");
    out.extend(when(cache.time));
    out.push(b'\n');
    if let Some(m) = mtime.filter(|&m| m > cache.time + 1) {
        out.extend_from_slice(b"  last checked ");
        out.extend(when(m));
        out.push(b'\n');
    }
    let mut diffs = Vec::new();
    let merged = if cache.build != new.build {
        diffs.push(format!(
            "built by another build of luish ({})",
            String::from_utf8_lossy(&cache.build)
        ));
        new
    } else if cache.dir != new.dir {
        diffs.push(format!("built from {}", String::from_utf8_lossy(&cache.dir)));
        new
    } else {
        diff_caches(&cache, &new, &mut diffs);
        let mut merged = cache;
        // The files that the run didn't reach are gone.
        let files: Vec<Vec<u8>> = (new.entries.iter())
            .filter(|e| e.id.starts_with(b"file "))
            .map(|e| e.id.clone())
            .collect();
        merged
            .entries
            .retain(|e| !e.id.starts_with(b"file ") || files.contains(&e.id));
        for e in new.entries {
            merged.store(e);
        }
        merged.time = new.time;
        merged.mode = new.mode;
        merged.env = new.env;
        merged
    };
    let changed = !diffs.is_empty();
    if changed {
        for d in &diffs {
            out.extend_from_slice(format!("  {d}\n").as_bytes());
        }
        write_cache(path, &merged.serialize()).map_err(|e| format!("cannot write the cache: {}", sys::strerror(e)))?;
        out.extend_from_slice(b"  rebuilt\n");
    } else {
        sys::touch(path).map_err(|e| format!("cannot touch the cache: {}", sys::strerror(e)))?;
        out.extend_from_slice(b"  up to date\n");
    }
    Ok((changed, out))
}

/// What an entry is for, in a report.
fn label(e: &Entry) -> String {
    if let Some(f) = e.id.strip_prefix(b"file ") {
        return String::from_utf8_lossy(f).into_owned();
    }
    if let Some(b) = e.id.strip_prefix(b"block ") {
        let path = b.get(17..).unwrap_or_default();
        return format!("block at {}:{}", String::from_utf8_lossy(path), e.line);
    }
    String::from_utf8_lossy(match &e.id[..] {
        b"config" => b"config.toml",
        b"post-rc" => b"post-rc.lsh",
        id => id,
    })
    .into_owned()
}

/// The differences between the entries of the cache and those that
/// running the files again gave.
fn diff_caches(old: &Cache, new: &Cache, diffs: &mut Vec<String>) {
    for n in &new.entries {
        let what = label(n);
        if let Some(o) = old.entries.iter().find(|o| o.id == n.id && o.key == n.key) {
            diff_deps(&o.deps, &n.deps, diffs);
            let decode = |e: &Entry| decode_changes(&e.changes).unwrap_or_default();
            diff_changes(&what, &decode(o), &decode(n), diffs);
            continue;
        }
        let Some(o) = (old.entries.iter().filter(|o| o.id == n.id)).max_by_key(|o| o.time) else {
            diffs.push(match n.id.starts_with(b"file ") {
                true => format!("file added: {what}"),
                false => format!("{what}: added"),
            });
            continue;
        };
        // Only what differs in the entry's own key: a change earlier in the
        // chain is reported for its own entry.
        let old_items: Vec<_> = items(&o.key).collect();
        for (tag, value) in items(&n.key) {
            if old_items.contains(&(tag, value)) {
                continue;
            }
            match tag {
                b'f' | b'm' => diffs.push(format!("file changed: {what}")),
                b'v' | b'u' => {
                    let name = value.split(|&c| c == b'=').next().unwrap_or_default();
                    diffs.push(format!("{what}: variable {}: different value", printable(name)));
                }
                b'p' => {
                    let path = value.splitn(6, |&c| c == b' ').nth(5).unwrap_or_default();
                    diffs.push(format!("{what}: file changed: {}", String::from_utf8_lossy(path)));
                }
                _ => {}
            }
        }
    }
    for o in &old.entries {
        if let Some(f) = o.id.strip_prefix(b"file ")
            && !new.entries.iter().any(|n| n.id == o.id)
        {
            let d = format!("file removed: {}", String::from_utf8_lossy(f));
            if !diffs.contains(&d) {
                diffs.push(d);
            }
        }
    }
}

/// Starts a shell as the one that built `cache`, to check it
/// ([`check_child`]), and returns what it wrote to `tmp`. Its input and
/// output are `/dev/null`, so what the files print isn't shown.
fn rerun(sh: &mut Shell, name: &[u8], cache: &Cache, tmp: &[u8]) -> Result<Vec<u8>, String> {
    let exe = sh.self_exe();
    let argv = [
        b"luish".to_vec(),
        cache.mode.clone(),
        // Without job control, so that it leaves the terminal alone.
        b"+m".to_vec(),
        [b"--internal-check-cache=", name, b":", tmp].concat(),
    ];
    let env: Vec<CString> = (cache.env.split(|&c| c == 0))
        .filter(|e| !e.is_empty())
        .filter_map(|e| CString::new(e).ok())
        .collect();
    let pid = sh.fork_or_error().map_err(|_| "cannot fork".to_string())?;
    if pid == 0 {
        if let Ok(null) = sys::open(b"/dev/null", libc::O_RDWR, 0) {
            for fd in 0..3 {
                let _ = sys::dup2(null, fd);
            }
            sys::close(null);
        }
        sys::execve(&exe, &argv, &env);
        sys::exit(127);
    }
    let status = sh.wait_for(pid);
    match std::fs::read(to_path(tmp)) {
        Ok(t) if !t.is_empty() => Ok(t),
        _ => Err(format!("the shell that rebuilds it failed (status {status})")),
    }
}

/// The files that changed, were added or were removed.
fn diff_deps(old: &[Dep], new: &[Dep], diffs: &mut Vec<String>) {
    for d in old {
        let path = String::from_utf8_lossy(&d.path);
        match new.iter().find(|e| e.path == d.path) {
            None => diffs.push(format!("file removed: {path}")),
            Some(n) if n.stamp != d.stamp => diffs.push(format!("file changed: {path}")),
            Some(_) => {}
        }
    }
    for d in new.iter().filter(|d| !old.iter().any(|e| e.path == d.path)) {
        diffs.push(format!("file added: {}", String::from_utf8_lossy(&d.path)));
    }
}

/// The differences between what an entry of the cache changes and what
/// running it again changed, in the order of the state.
fn diff_changes(what: &str, old: &[Change], new: &[Change], diffs: &mut Vec<String>) {
    let label = |c: &Change| {
        let kind = c.kind.label();
        match &c.name[..] {
            [] => format!("{what}: {kind}"),
            name => format!("{what}: {kind} {}", printable(name)),
        }
    };
    let find = |list: &[Change], c: &Change| list.iter().position(|o| o.kind == c.kind && o.name == c.name);
    for c in new {
        match find(old, c) {
            None if c.removed => diffs.push(format!("{}: removed", label(c))),
            None => diffs.push(format!("{}: added", label(c))),
            Some(i) if old[i] != *c => diffs.push(format!("{}: changed", label(c))),
            Some(_) => {}
        }
    }
    // What the cache restores that running it no longer changes: it keeps
    // its value from before the entry.
    for c in old.iter().filter(|c| find(new, c).is_none()) {
        let what = match c.kind {
            _ if c.removed => "added",
            Kind::Dir | Kind::Umask | Kind::SyntaxOption | Kind::Option => "changed",
            _ => "removed",
        };
        diffs.push(format!("{}: {what}", label(c)));
    }
}

/// A name for a report, with control characters (in key sequences) as `^X`.
fn printable(name: &[u8]) -> String {
    let mut out = Vec::new();
    for &c in name {
        match c {
            0..0x20 | 0x7f => out.extend_from_slice(&[b'^', c ^ 0x40]),
            _ => out.push(c),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The locale for times, from the shell's variables.
fn time_locale(sh: &Shell) -> Vec<u8> {
    [&b"LC_ALL"[..], b"LC_TIME", b"LANG"]
        .iter()
        .find_map(|v| sh.get_var(v).filter(|l| !l.is_empty()))
        .unwrap_or_else(|| b"C".to_vec())
}

/// How long `secs` seconds is, as " (N units ago)".
fn ago(secs: i64) -> String {
    let (n, unit) = match secs {
        ..0 => return String::new(),
        0..60 => return " (just now)".to_string(),
        60..3600 => (secs / 60, "minute"),
        3600..86400 => (secs / 3600, "hour"),
        _ => (secs / 86400, "day"),
    };
    format!(" ({n} {unit}{} ago)", if n == 1 { "" } else { "s" })
}
