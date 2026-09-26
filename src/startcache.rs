//! Cached startup files (a first version of PLAN.md §9.2).
//!
//! Two directories in `$XDG_CONFIG_HOME/luish/` hold `*.lsh` files whose
//! effects are cached, as zsh's `.zshrc` and `.zlogin`: `rc.d/`, for every
//! interactive shell, and `login.d/`, for login shells, after `rc.d` (and
//! instead of `/etc/profile` and `~/.profile`). Each directory's files run
//! in byte order. What they change (variables, functions, aliases, options,
//! traps, `umask` and the directory) is saved as commands in
//! `$XDG_CACHE_HOME/luish/NAME-HOST` (`rc-HOST`, `login-HOST`), with the
//! fingerprints of the files and of every file they sourced with `.`. Later
//! shells check the fingerprints and run the saved commands instead of the
//! files; when a fingerprint differs, they rerun the files and rewrite the
//! cache. A directory's `_uncached.lsh` runs every time, after the rest.
//!
//! Not yet done (see the plan): keying the cache on the variables the files
//! read, so the saved values are those of the environment the cache was
//! built in; noticing changes that a fingerprint can't show (the output of
//! commands, files tested with `[`); and revalidating in the background.

use crate::interactive::{run_file, source_file};
use crate::shell::Shell;
use crate::{state, sys};

/// Bumped when the format of the cache changes.
const HEADER: &[u8] = b"# luish startup cache 1\n";
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

/// A cache file: the directory it was built from, what it depends on, and
/// the commands that restore the state.
struct Cache {
    dir: Vec<u8>,
    deps: Vec<Dep>,
    state: Vec<u8>,
}

impl Cache {
    fn parse(text: &[u8]) -> Option<Cache> {
        let mut rest = text.strip_prefix(HEADER)?;
        let mut dir = None;
        let mut deps = Vec::new();
        loop {
            if let Some(state) = rest.strip_prefix(STATE_MARK) {
                return Some(Cache {
                    dir: dir?,
                    deps,
                    state: state.to_vec(),
                });
            }
            let nl = rest.iter().position(|&c| c == b'\n')?;
            let line = &rest[..nl];
            rest = &rest[nl + 1..];
            match line.split_first()? {
                (b'd', path) => dir = Some(path.strip_prefix(b" ")?.to_vec()),
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
        out.extend_from_slice(b"d ");
        out.extend_from_slice(&self.dir);
        out.push(b'\n');
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
    let mut buf = [0u8; 256];
    // SAFETY: gethostname into a buffer of the given size.
    let r = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() - 1) };
    let name: Vec<u8> = if r == 0 {
        buf.iter()
            .take_while(|&&c| c != 0)
            .map(|&c| if c == b'/' { b'_' } else { c })
            .collect()
    } else {
        Vec::new()
    };
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

/// Whether the cache was built from `dir` and the files it depends on
/// haven't changed since.
fn is_current(cache: &Cache, dir: &[u8], files: &[Dep]) -> bool {
    let mut cached = cache.deps.iter().filter(|d| !d.sourced);
    cache.dir == dir
        && files.iter().all(|f| cached.next() == Some(f))
        && cached.next().is_none()
        && cache
            .deps
            .iter()
            .filter(|d| d.sourced)
            .all(|d| format_stamp(&stamp(&d.path)) == d.stamp)
}

/// Runs the startup files in `dir`, from the cache `luish/NAME-HOST` when it
/// is current.
pub fn run(sh: &mut Shell, dir: &[u8], name: &[u8]) {
    let files = cached_files(dir);
    let cache_path = xdg_dir(sh, b"XDG_CACHE_HOME", b"/.cache").map(|mut c| {
        c.extend_from_slice(b"/luish/");
        c.extend_from_slice(name);
        c.push(b'-');
        c.extend(hostname());
        c
    });
    let cache = cache_path
        .as_ref()
        .and_then(|p| std::fs::read(crate::interactive::to_path(p)).ok())
        .and_then(|t| Cache::parse(&t))
        .filter(|c| is_current(c, dir, &files));
    match cache {
        Some(c) => run_file(sh, &c.state),
        None => build(sh, dir, files, cache_path.as_deref()),
    }
    let uncached = join(dir, b"_uncached.lsh");
    if sys::stat(&uncached).is_some() {
        source_file(sh, &uncached);
    }
}

/// Runs the files, and saves what they changed.
fn build(sh: &mut Shell, dir: &[u8], files: Vec<Dep>, cache_path: Option<&[u8]>) {
    let before = sh.state_entries();
    sh.sourced_files = Some(Vec::new());
    for f in &files {
        source_file(sh, &join(dir, &f.path));
    }
    let sourced = sh.sourced_files.take().unwrap_or_default();
    let Some(cache_path) = cache_path else { return };
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
        return;
    }
    let cache = Cache {
        dir: dir.to_vec(),
        deps,
        state: state::difference(&before, &sh.state_entries()),
    };
    if let Err(e) = write_cache(cache_path, &cache.serialize()) {
        sh.error(format!(
            "cannot write startup cache {}: {}",
            String::from_utf8_lossy(cache_path),
            sys::strerror(e)
        ));
    }
}

/// Writes the cache with mode 0600 (exported variables can hold tokens),
/// through a temporary file renamed into place.
fn write_cache(path: &[u8], text: &[u8]) -> Result<(), i32> {
    use std::os::unix::fs::DirBuilderExt;
    let slash = path.iter().rposition(|&c| c == b'/').unwrap_or(0);
    let parent = crate::interactive::to_path(&path[..slash]);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&parent)
        .map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))?;
    let (fd, tmp) = sys::mkstemp(&[path, b"."].concat())?;
    let ok = sys::write_all(fd, text);
    let e = sys::errno();
    sys::close(fd);
    let r = if !ok {
        Err(e)
    } else {
        std::fs::rename(crate::interactive::to_path(&tmp), crate::interactive::to_path(path))
            .map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))
    };
    if r.is_err() {
        sys::unlink(&tmp);
    }
    r
}
