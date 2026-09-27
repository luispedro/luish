//! Cached startup files (a first version of the Stage 3 design in
//! PLAN.md).
//!
//! Two directories in `$XDG_CONFIG_HOME/luish/` hold `*.lsh` files whose
//! effects are cached, as zsh's `.zshrc` and `.zlogin`: `rc.d/`, for every
//! interactive shell, and `login.d/`, for login shells, after `rc.d` (and
//! instead of `/etc/profile` and `~/.profile`). Each directory's files run
//! in byte order. What they change (variables, functions, aliases, options,
//! traps, `umask` and the directory) is saved as commands in
//! `$XDG_CACHE_HOME/luish/NAME-HOST` (`rc-HOST`, `login-HOST`), with the
//! fingerprints of the files and of every file they sourced with `.`, the
//! build of luish that wrote it, and, for `__luish_internal check-cache`,
//! when it was built and the environment and options of the shell that
//! built it. `rc.d`'s cache also covers
//! `config.toml` (see `config.rs`), applied before its files and
//! fingerprinted even when it doesn't exist, so that creating it is
//! noticed, and the plugins it enables (`plugins/package.rs`), loaded
//! before the files (with `plugins.lock` and the manifests of local plugins
//! in the key); the plugins' `post-rc.lsh` files run after them, and their
//! `post-rc` hooks after the cached state is restored or built (before
//! `_uncached.lsh`). With `config.toml`, `rc.d`'s cache is used even if the directory
//! doesn't exist. `--no-plugins` bypasses the caches. Later shells check the fingerprints and the build and run the
//! saved commands instead of the files; when either differs, they rerun
//! the files and rewrite the cache. A directory's `_uncached.lsh` runs
//! every time, after the rest.
//!
//! `__luish_internal check-cache` ([`check`]) looks for the changes that
//! the fingerprints miss: it starts a shell as the one that built the cache
//! (its options and environment), which restores the cache in a forked copy
//! of itself, runs the files itself ([`check_child`]), and reports both
//! states and the new cache. If they are the same, the cache is only
//! touched (its modification time is when it was last found current);
//! otherwise it is replaced.
//!
//! Not yet done (see the plan): keying the cache on the variables the files
//! read, so the saved values are those of the environment the cache was
//! built in; noticing changes that a fingerprint can't show (the output of
//! commands, files tested with `[`) other than by `check-cache`; and
//! revalidating in the background.

use std::ffi::CString;

use crate::builtins::internal::BUILD_ID;
use crate::interactive::{run_file, source_file, to_path};
use crate::shell::{ExecResult, Shell};
use crate::{state, sys};

/// Bumped when the format of the cache changes.
const HEADER: &[u8] = b"# luish startup cache 3\n";
/// Separates the key from the saved state.
const STATE_MARK: &[u8] = b"# state\n";

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

/// A file the cache depends on: a file in the directory (by name), or a file
/// that one of them sourced (by absolute path).
#[derive(Debug, PartialEq)]
struct Dep {
    sourced: bool,
    path: Vec<u8>,
    stamp: String,
}

/// A cache file: the build of luish that wrote it, the directory it was
/// built from, when and in what environment, what it depends on, and the
/// commands that restore the state.
struct Cache {
    build: Vec<u8>,
    dir: Vec<u8>,
    /// When it was built, in seconds since the epoch (UTC).
    time: i64,
    /// The options that start a shell like the one that built it: `-i`,
    /// `-l` or `-il`.
    mode: Vec<u8>,
    /// The environment that shell started with: `NAME=VALUE` strings, each
    /// followed by a NUL (in the file, `e LEN` lines, each followed by the
    /// string and a newline).
    env: Vec<u8>,
    deps: Vec<Dep>,
    state: Vec<u8>,
}

impl Cache {
    fn parse(text: &[u8]) -> Option<Cache> {
        let mut rest = text.strip_prefix(HEADER)?;
        let mut build = None;
        let mut dir = None;
        let mut time = None;
        let mut mode = None;
        let mut env = Vec::new();
        let mut deps = Vec::new();
        loop {
            if let Some(state) = rest.strip_prefix(STATE_MARK) {
                return Some(Cache {
                    build: build?,
                    dir: dir?,
                    time: time?,
                    mode: mode?,
                    env,
                    deps,
                    state: state.to_vec(),
                });
            }
            let nl = rest.iter().position(|&c| c == b'\n')?;
            let line = &rest[..nl];
            rest = &rest[nl + 1..];
            match line.split_first()? {
                (b'b', id) => build = Some(id.strip_prefix(b" ")?.to_vec()),
                (b'd', path) => dir = Some(path.strip_prefix(b" ")?.to_vec()),
                (b't', t) => time = Some(std::str::from_utf8(t.strip_prefix(b" ")?).ok()?.parse().ok()?),
                (b'm', m) => mode = Some(m.strip_prefix(b" ")?.to_vec()),
                // `e LEN`, then a variable of LEN bytes and a newline.
                (b'e', len) => {
                    let len: usize = std::str::from_utf8(len.strip_prefix(b" ")?).ok()?.parse().ok()?;
                    env.extend_from_slice(rest.get(..len)?);
                    env.push(0);
                    rest = rest.get(len..)?.strip_prefix(b"\n")?;
                }
                (&c @ (b'l' | b's'), fields) => {
                    // `l DEV INO SIZE SEC NSEC NAME`: five fields, then the name.
                    let mut parts = fields.strip_prefix(b" ")?.splitn(6, |&c| c == b' ');
                    let stamp: Vec<&[u8]> = parts.by_ref().take(5).collect();
                    let path = parts.next()?.to_vec();
                    deps.push(Dep {
                        sourced: c == b's',
                        path,
                        stamp: String::from_utf8(stamp.join(&b' ')).ok()?,
                    });
                }
                _ => return None,
            }
        }
    }

    fn serialize(&self) -> Vec<u8> {
        let mut out = HEADER.to_vec();
        out.extend_from_slice(b"b ");
        out.extend_from_slice(&self.build);
        out.push(b'\n');
        out.extend_from_slice(b"d ");
        out.extend_from_slice(&self.dir);
        out.push(b'\n');
        out.extend_from_slice(format!("t {}\nm ", self.time).as_bytes());
        out.extend_from_slice(&self.mode);
        out.push(b'\n');
        for var in self.env.split(|&c| c == 0).filter(|v| !v.is_empty()) {
            out.extend_from_slice(format!("e {}\n", var.len()).as_bytes());
            out.extend_from_slice(var);
            out.push(b'\n');
        }
        for d in &self.deps {
            out.extend_from_slice(if d.sourced { b"s " } else { b"l " });
            out.extend_from_slice(d.stamp.as_bytes());
            out.push(b' ');
            out.extend_from_slice(&d.path);
            out.push(b'\n');
        }
        out.extend_from_slice(STATE_MARK);
        out.extend_from_slice(&self.state);
        out
    }
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
fn cached_files(dir: &[u8]) -> Vec<Dep> {
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
            (st.st_mode & libc::S_IFMT == libc::S_IFREG).then(|| Dep {
                sourced: false,
                stamp: format_stamp(&Some((st.st_dev, st.st_ino, st.st_size, st.st_mtime, st.st_mtime_nsec))),
                path: name,
            })
        })
        .collect()
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    [dir, b"/", name].concat()
}

/// Whether the cache was written by this build of luish, from `dir`, and
/// the files it depends on haven't changed since.
fn is_current(cache: &Cache, dir: &[u8], files: &[Dep]) -> bool {
    let mut cached = cache.deps.iter().filter(|d| !d.sourced);
    cache.build == BUILD_ID.as_bytes()
        && cache.dir == dir
        && files.iter().all(|f| cached.next() == Some(f))
        && cached.next().is_none()
        && cache
            .deps
            .iter()
            .filter(|d| d.sourced)
            .all(|d| format_stamp(&stamp(&d.path)) == d.stamp)
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

/// Runs the startup files in `dir`, from the cache `luish/NAME-HOST` when it
/// is current. `config` is `config.toml`, applied first and cached with
/// the files (whether it exists or not, so that creating it is noticed).
pub fn run(sh: &mut Shell, dir: &[u8], name: &[u8], config: Option<&[u8]>) {
    let files = cached_files(dir);
    // A shell without plugins would save a state without them.
    let cache_path = cache_file(sh, name).filter(|_| !sh.no_plugins);
    if sh.check_cache.as_ref().is_some_and(|c| c.0 == name) {
        check_child(sh, dir, name, files, config);
    }
    let cache = cache_path
        .as_ref()
        .and_then(|p| std::fs::read(to_path(p)).ok())
        .and_then(|t| Cache::parse(&t))
        .filter(|c| is_current(c, dir, &files));
    let rc = config.is_some();
    sh.in_rc = rc;
    match cache {
        Some(c) => run_file(sh, &c.state),
        None => {
            let built = build(sh, dir, name, files, config, cache_path.is_some());
            if let (Ok(cache), Some(path)) = (built, &cache_path) {
                save(sh, path, &cache.serialize());
            }
        }
    }
    sh.in_rc = false;
    // Extensions run in every shell, from the cache too.
    if rc {
        crate::plugins::post_rc_hooks(sh);
    }
    // Not while checking a cache: it is for side effects.
    let uncached = join(dir, b"_uncached.lsh");
    if sh.check_cache.is_none() && sys::stat(&uncached).is_some() {
        source_file(sh, &uncached);
    }
}

/// Runs the files, and returns the cache of what they changed (if `save`,
/// otherwise an error), or why it can't be saved.
fn build(
    sh: &mut Shell,
    dir: &[u8],
    name: &[u8],
    files: Vec<Dep>,
    config: Option<&[u8]>,
    save: bool,
) -> Result<Cache, &'static str> {
    let before = sh.state_entries();
    if let Some(c) = config {
        crate::config::load(sh, c);
    }
    sh.sourced_files = Some(config.into_iter().map(<[u8]>::to_vec).collect());
    // Plugins that aren't installed are reported by every shell until they
    // are.
    let complete = config.is_none() || crate::plugins::load_enabled(sh);
    for f in &files {
        source_file(sh, &join(dir, &f.path));
    }
    if config.is_some() {
        crate::plugins::post_rc_files(sh);
    }
    let sourced = sh.sourced_files.take().unwrap_or_default();
    if !save {
        return Err("not saved");
    }
    if !complete {
        return Err("a plugin that config.toml enables isn't installed");
    }
    let mut deps = files;
    for path in sourced {
        if !deps.iter().any(|d| d.sourced && d.path == path) {
            deps.push(Dep {
                sourced: true,
                stamp: format_stamp(&stamp(&path)),
                path,
            });
        }
    }
    // A name with a newline can't be written in the key.
    if dir.contains(&b'\n') || deps.iter().any(|d| d.path.contains(&b'\n')) {
        return Err("a file name has a newline");
    }
    let mode: &[u8] = match (name, sh.interactive) {
        (b"login", true) => b"-il",
        (b"login", false) => b"-l",
        _ => b"-i",
    };
    Ok(Cache {
        build: BUILD_ID.as_bytes().to_vec(),
        dir: dir.to_vec(),
        time: sys::now(),
        mode: mode.to_vec(),
        env: environment(),
        deps,
        state: state::difference(&before, &sh.state_entries()),
    })
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

/// Writes a cache, reporting failures.
fn save(sh: &Shell, path: &[u8], text: &[u8]) {
    if let Err(e) = write_cache(path, text) {
        sh.error(format!(
            "cannot write startup cache {}: {}",
            String::from_utf8_lossy(path),
            sys::strerror(e)
        ));
    }
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
  functions, aliases, options, ...), saved on the host HOST, so that new
  shells restore them instead of running the files. Each also holds the
  environment of the shell that built it, so that
  `__luish_internal check-cache` can run the files again as it did.
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

/// The shell's state, as entries for [`check`] to compare: the kind, the
/// name and the commands of each.
fn entries(sh: &Shell) -> Vec<u8> {
    let mut out = Vec::new();
    for e in sh.state_entries() {
        put(&mut out, e.kind.label().as_bytes());
        put(&mut out, &e.name);
        put(&mut out, &e.text);
    }
    out
}

/// In the shell that [`check`] starts, at the cache it checks (instead of
/// using the cache): restores the state that `check` wrote to the file it
/// named (the cache's) in a forked copy of the shell, then runs the files,
/// and writes to the same file the state after each, and the new cache. Or
/// why it couldn't, if the files didn't complete.
fn check_child(sh: &mut Shell, dir: &[u8], name: &[u8], files: Vec<Dep>, config: Option<&[u8]>) -> ! {
    let (_, out) = sh.check_cache.take().unwrap_or_default();
    let cached = std::fs::read(to_path(&out)).unwrap_or_default();
    sh.in_rc = config.is_some();
    // The copy passes its state back through the file.
    let _ = std::fs::write(to_path(&out), b"");
    let restored = match sys::fork() {
        Ok(0) => {
            run_file(sh, &cached);
            let _ = std::fs::write(to_path(&out), entries(sh));
            sys::exit(0)
        }
        Ok(pid) => {
            sh.wait_for(pid);
            std::fs::read(to_path(&out)).ok().filter(|r| !r.is_empty())
        }
        Err(_) => None,
    };
    let mut report = Vec::new();
    match (restored, build(sh, dir, name, files, config, true)) {
        (None, _) => put(&mut report, b"cannot restore it"),
        (_, Err(msg)) => put(&mut report, msg.as_bytes()),
        (Some(restored), Ok(cache)) => {
            put(&mut report, b"ok");
            put(&mut report, &restored);
            put(&mut report, &entries(sh));
            put(&mut report, &cache.serialize());
        }
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

/// `__luish_internal check-cache [-q] [rc|login]...`: checks the startup
/// caches (those of this host that exist, by default) by running their
/// files again. Status 0 if they were current (they are then touched),
/// 1 if any was replaced, 2 on errors. The report is printed only for the
/// caches that were replaced, with `-q`.
pub fn check(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut quiet = false;
    let mut names: Vec<&[u8]> = Vec::new();
    let mut options = true;
    for a in &argv[1..] {
        match a.as_slice() {
            b"-q" | b"--quiet" if options => quiet = true,
            b"--" if options => options = false,
            b"rc" | b"login" => {
                options = false;
                names.push(a);
            }
            _ if options && a.first() == Some(&b'-') => {
                sh.berr(CHECK, format!("illegal option {}", String::from_utf8_lossy(a)));
                return Ok(2);
            }
            _ => {
                let msg = format!("{}: no such cache (rc or login)", String::from_utf8_lossy(a));
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
        names = vec![b"rc", b"login"];
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
        match check_one(sh, name, &path, &cache) {
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
fn check_one(sh: &mut Shell, name: &[u8], path: &[u8], cache: &Cache) -> Result<(bool, Vec<u8>), String> {
    let mtime = sys::stat(path).map(|st| st.st_mtime);
    let (fd, tmp) = sys::mkstemp(&[path, b".check."].concat())
        .map_err(|e| format!("cannot create a temporary file: {}", sys::strerror(e)))?;
    let written = sys::write_all(fd, &cache.state);
    let e = sys::errno();
    sys::close(fd);
    let result = match written {
        true => rerun(sh, name, cache, &tmp),
        false => Err(format!("cannot write a temporary file: {}", sys::strerror(e))),
    };
    sys::unlink(&tmp);
    let result = result?;
    let mut rest = result.as_slice();
    let bad = || "the check failed".to_string();
    let status = take(&mut rest).ok_or_else(bad)?;
    if status != b"ok" {
        return Err(format!("cannot rebuild it: {}", String::from_utf8_lossy(status)));
    }
    let restored = take(&mut rest).ok_or_else(bad)?;
    let rebuilt = take(&mut rest).ok_or_else(bad)?;
    let new_text = take(&mut rest).ok_or_else(bad)?;
    let new = Cache::parse(new_text).ok_or_else(bad)?;

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
    if cache.build != new.build {
        diffs.push(format!(
            "built by another build of luish ({})",
            String::from_utf8_lossy(&cache.build)
        ));
    }
    if cache.dir != new.dir {
        diffs.push(format!("built from {}", String::from_utf8_lossy(&cache.dir)));
    }
    diff_deps(&cache.deps, &new.deps, &mut diffs);
    diff_entries(restored, rebuilt, &mut diffs);
    let changed = !diffs.is_empty();
    if changed {
        for d in &diffs {
            out.extend_from_slice(format!("  {d}\n").as_bytes());
        }
        write_cache(path, new_text).map_err(|e| format!("cannot write the cache: {}", sys::strerror(e)))?;
        out.extend_from_slice(b"  rebuilt\n");
    } else {
        sys::touch(path).map_err(|e| format!("cannot touch the cache: {}", sys::strerror(e)))?;
        out.extend_from_slice(b"  up to date\n");
    }
    Ok((changed, out))
}

/// Starts a shell as the one that built `cache`, to check it
/// ([`check_child`]) with its state in `tmp`, and returns what it wrote
/// there. Its input and output
/// are `/dev/null`, so what the files print isn't shown.
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
    let find = |deps: &[Dep], d: &Dep| deps.iter().position(|e| e.sourced == d.sourced && e.path == d.path);
    for d in old {
        let path = String::from_utf8_lossy(&d.path);
        match find(new, d) {
            None => diffs.push(format!("file removed: {path}")),
            Some(i) if new[i].stamp != d.stamp => diffs.push(format!("file changed: {path}")),
            Some(_) => {}
        }
    }
    for d in new.iter().filter(|d| find(old, d).is_none()) {
        diffs.push(format!("file added: {}", String::from_utf8_lossy(&d.path)));
    }
}

/// The differences between the state that the cache restores and the one
/// that running the files gives, in the order of the state.
fn diff_entries<'a>(restored: &'a [u8], rebuilt: &'a [u8], diffs: &mut Vec<String>) {
    use std::collections::HashMap;
    fn parse(mut b: &[u8]) -> Vec<(&[u8], &[u8], &[u8])> {
        let mut out = Vec::new();
        while !b.is_empty() {
            let (Some(kind), Some(name), Some(text)) = (take(&mut b), take(&mut b), take(&mut b)) else {
                break;
            };
            out.push((kind, name, text));
        }
        out
    }
    let (old, new) = (parse(restored), parse(rebuilt));
    let key = |&(kind, name, _): &(&'a [u8], &'a [u8], &'a [u8])| (kind, name);
    let old_map: HashMap<_, _> = old.iter().map(|e| (key(e), e.2)).collect();
    let new_map: HashMap<_, _> = new.iter().map(|e| (key(e), e.2)).collect();
    let label = |(kind, name): (&[u8], &[u8])| {
        let kind = String::from_utf8_lossy(kind);
        match name {
            [] => kind.into_owned(),
            _ => format!("{kind} {}", printable(name)),
        }
    };
    for e in &new {
        match old_map.get(&key(e)) {
            None => diffs.push(format!("{}: added", label(key(e)))),
            Some(&text) if text != e.2 => diffs.push(format!("{}: changed", label(key(e)))),
            Some(_) => {}
        }
    }
    for e in old.iter().filter(|e| !new_map.contains_key(&key(e))) {
        diffs.push(format!("{}: removed", label(key(e))));
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
