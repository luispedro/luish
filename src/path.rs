//! `PATH` search, with a cache of found commands (`hash`).
//!
//! An interactive shell also notices changes to the `PATH` directories
//! (`check_path_dirs`), so that the cache never hides a newly installed
//! command, as it does in other shells until `hash -r`.

use crate::shell::Shell;
use crate::sys;

/// The default search path (for `command -p`), as in dash.
pub const DEFAULT_PATH: &[u8] = b"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// `dir/name`, with an empty `dir` meaning the current directory.
fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut cand = if dir.is_empty() { b".".to_vec() } else { dir.to_vec() };
    cand.push(b'/');
    cand.extend_from_slice(name);
    cand
}

/// Whether `path` is a regular file that can be executed: what a `PATH`
/// search takes as a command.
pub fn is_executable(path: &[u8]) -> bool {
    is_regular(path) && sys::access(path, libc::X_OK)
}

fn is_regular(path: &[u8]) -> bool {
    sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG)
}

/// The names of the executables in the directories of `path` (a `PATH`)
/// that start with `prefix`, sorted and without duplicates.
pub fn executables(path: &[u8], prefix: &[u8]) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    for dir in path.split(|&c| c == b':') {
        for name in sys::read_dir(dir).unwrap_or_default() {
            if name.starts_with(prefix) && name != b"." && name != b".." && is_executable(&join(dir, &name)) {
                names.push(name);
            }
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// Searches `path` for `name`. Returns the file and the index of its
/// directory in `path`, and whether it is executable. Falls back to a
/// non-executable regular file (so that exec reports "Permission denied").
pub fn search(path: &[u8], name: &[u8]) -> Option<(Vec<u8>, usize, bool)> {
    let mut fallback = None;
    for (i, dir) in path.split(|&c| c == b':').enumerate() {
        let cand = join(dir, name);
        // One `stat` per directory, and an `access` only for a regular file.
        if !is_regular(&cand) {
            continue;
        }
        if sys::access(&cand, libc::X_OK) {
            return Some((cand, i, true));
        }
        if fallback.is_none() {
            fallback = Some((cand, i, false));
        }
    }
    fallback
}

/// What a `PATH` directory looks like: device, inode and modification time.
/// The inode catches a directory reached through a symlink that now points
/// elsewhere, as when a Nix profile is switched (Nix store directories all
/// have the modification time 1).
pub type DirStamp = Option<(u64, u64, i64, i64)>;

pub fn dir_stamps(path: &[u8]) -> Vec<DirStamp> {
    path.split(|&c| c == b':')
        .map(|d| {
            sys::stat(if d.is_empty() { b"." } else { d })
                .map(|st| (st.st_dev, st.st_ino, st.st_mtime, st.st_mtime_nsec))
        })
        .collect()
}

impl Shell {
    /// Forgets the remembered commands if a `PATH` directory changed since
    /// the last call. The interactive shell calls this for each line read, so
    /// that a command installed meanwhile is found even if it shadows a
    /// remembered one.
    pub fn check_path_dirs(&mut self) {
        let stamps = dir_stamps(&self.get_var(b"PATH").unwrap_or_default());
        if stamps != self.path_stamps {
            self.hash.clear();
            self.path_stamps = stamps;
        }
    }

    /// Finds a command in `PATH`, and the index of its directory there. As
    /// in dash, a command in the cache is trusted without checking the file
    /// (see [`Shell::with_command_path`] for when it is gone).
    pub fn find_in_path(&mut self, name: &[u8]) -> Option<(Vec<u8>, usize)> {
        if let Some(p) = self.hash.get(name) {
            return Some(p.clone());
        }
        let (found, i, exec) = search(&self.get_var(b"PATH").unwrap_or_default(), name)?;
        if exec {
            self.hash.insert(name.to_vec(), (found.clone(), i));
        }
        Some((found, i))
    }

    /// The file that running `name` would execute, as `command -v` finds it
    /// but never a function or built-in: `name` itself if it has a `/`,
    /// else the executable found in `PATH` (a remembered one if it is
    /// still there). `None` if there is none.
    #[cfg(feature = "plugins")]
    pub fn which(&mut self, name: &[u8]) -> Option<Vec<u8>> {
        if name.contains(&b'/') {
            return is_executable(name).then(|| name.to_vec());
        }
        if name.is_empty() {
            return None;
        }
        if let Some((found, _)) = self.find_in_path(name)
            && is_executable(&found)
        {
            return Some(found);
        }
        // A remembered command that is gone, or no executable file.
        self.hash.remove(name);
        let (found, i, exec) = search(&self.get_var(b"PATH").unwrap_or_default(), name)?;
        exec.then(|| {
            self.hash.insert(name.to_vec(), (found.clone(), i));
            found
        })
    }

    /// Runs `f` (which execs or spawns) on the file for command `name`. As
    /// dash's `shellexec` does, if the file is gone (a cached command that
    /// was removed), `f` is tried on `name` in the `PATH` directories after
    /// it. Returns what `f` returns, or an errno (`ENOENT` if not found).
    /// With `alt_path` (`command -p`), that path is searched instead of
    /// `PATH`, without the cache.
    pub fn with_command_path<T>(
        &mut self,
        name: &[u8],
        alt_path: Option<&[u8]>,
        mut f: impl FnMut(&mut Shell, &[u8]) -> Result<T, i32>,
    ) -> Result<T, i32> {
        if name.contains(&b'/') {
            return f(self, name);
        }
        let (path, idx) = match alt_path {
            Some(p) => search(p, name).map(|(f, i, _)| (f, i)),
            None => self.find_in_path(name),
        }
        .ok_or(libc::ENOENT)?;
        let mut e = match f(self, &path) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        if e == libc::ENOENT || e == libc::ENOTDIR {
            let path_var = alt_path.map_or_else(|| self.get_var(b"PATH").unwrap_or_default(), |p| p.to_vec());
            for dir in path_var.split(|&c| c == b':').skip(idx + 1) {
                match f(self, &join(dir, name)) {
                    Ok(v) => return Ok(v),
                    Err(x) if x != libc::ENOENT && x != libc::ENOTDIR => e = x,
                    Err(_) => {}
                }
            }
        }
        Err(e)
    }
}
