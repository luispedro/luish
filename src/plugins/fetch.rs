//! Git plugins (DEVELOPING.md): fetching with `git`, and the files of each
//! commit:
//!
//! - `$XDG_CACHE_HOME/luish/plugins/git/REPO-HASH/` (by default in
//!   `~/.cache`): a bare repository per URL, fetched into shallowly. Only
//!   `plugin sync` and `plugin update` use it, so it can be removed;
//! - `$XDG_DATA_HOME/luish/plugins/src/REPO-HASH/COMMIT/` (by default in
//!   `~/.local/share`): the files of one commit, from `git archive`,
//!   renamed into place once complete, so a directory that exists is
//!   complete. Startup needs these.
//!
//! Only `plugin sync`, `plugin update` and `plugin check` (which only runs
//! `git ls-remote`) come here. Startup only looks for the directories of the
//! locked commits, and never runs git.

use super::package::GitRef;
use crate::exec::shell_quote;
use crate::interactive::to_path;
use crate::shell::Shell;
use crate::sys;

/// Where plugins fetched with git are kept.
pub fn data_dir(sh: &Shell) -> Option<Vec<u8>> {
    let mut d = crate::startcache::xdg_dir(sh, b"XDG_DATA_HOME", b"/.local/share")?;
    d.extend_from_slice(b"/luish/plugins");
    Some(d)
}

/// Where the bare repositories are kept (they can be fetched again).
fn git_dir(sh: &Shell) -> Result<Vec<u8>, String> {
    let mut d = crate::startcache::cache_dir(sh).ok_or("cannot find the cache directory (HOME is not set)")?;
    d.extend_from_slice(b"/plugins/git");
    Ok(d)
}

const DATA_README: &[u8] = b"This directory holds the plugins that luish, the shell, fetched with git:
src/REPO-HASH/COMMIT/ has the files of one commit of a repository.

luish loads plugins from here when it starts, without fetching. If this
directory is removed, run `plugin sync` in luish to fetch them again.
";

/// `REPO-HASH`, which names the directories of a repository: the last part
/// of its URL, and a hash of the URL (FNV-1a, which doesn't change with the
/// version of Rust).
fn repo_key(url: &str) -> String {
    let base = url.trim_end_matches('/');
    let base = base.strip_suffix(".git").unwrap_or(base);
    let base: String = base
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_.".contains(*c))
        .collect();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let base = if base.is_empty() || base.starts_with('.') {
        "repo"
    } else {
        &base
    };
    format!("{base}-{h:016x}")
}

/// The directory with the files of `commit` of the repository at `url`.
pub fn checkout(data: &[u8], url: &str, commit: &str) -> Vec<u8> {
    [data, b"/src/", repo_key(url).as_bytes(), b"/", commit.as_bytes()].concat()
}

/// Runs git in the repository `dir`, and gives its output. Its errors go to
/// the shell's standard error.
fn git(sh: &mut Shell, dir: &[u8], args: &[&str]) -> Result<Vec<u8>, String> {
    // No prompt for a password: a mistyped repository on GitHub asks for
    // one. Credential helpers still work.
    let mut script = b"GIT_TERMINAL_PROMPT=0 command git -C ".to_vec();
    script.extend(shell_quote(dir));
    for a in args {
        script.push(b' ');
        script.extend(shell_quote(a.as_bytes()));
    }
    match super::capture(sh, &script) {
        Ok((0, out)) => Ok(out),
        Ok((s, _)) if s == 128 + libc::SIGINT => Err("interrupted".into()),
        Ok((s, _)) => Err(format!("git {} failed (status {s})", args[0])),
        Err(e) => Err(e.into()),
    }
}

/// The bare repository for `url`, created if it doesn't exist.
fn repository(sh: &mut Shell, url: &str) -> Result<Vec<u8>, String> {
    let dir = [git_dir(sh)?.as_slice(), b"/", repo_key(url).as_bytes()].concat();
    if !sys::is_dir(&dir) {
        std::fs::create_dir_all(to_path(&dir)).map_err(|e| format!("cannot create {}: {e}", show(&dir)))?;
        if let Some(cache) = crate::startcache::cache_dir(sh) {
            crate::startcache::mark_cache_dir(&to_path(&cache));
        }
        git(sh, &dir, &["init", "-q", "--bare"])?;
    }
    Ok(dir)
}

/// What to fetch for `at`.
fn refspec(at: &GitRef) -> String {
    match at {
        GitRef::Head => "HEAD".into(),
        GitRef::Branch(b) => format!("refs/heads/{b}"),
        GitRef::Tag(t) => format!("refs/tags/{t}"),
        GitRef::Rev(r) => r.clone(),
    }
}

/// Fetches the newest commit of `at` (a branch, a tag, `HEAD`) from `url`,
/// and gives its hash.
pub fn resolve(sh: &mut Shell, url: &str, at: &GitRef) -> Result<String, String> {
    let dir = repository(sh, url)?;
    git(
        sh,
        &dir,
        &["fetch", "-q", "--no-tags", "--depth", "1", url, &refspec(at)],
    )?;
    let out = git(sh, &dir, &["rev-parse", "--verify", "-q", "FETCH_HEAD^{commit}"])?;
    let commit = String::from_utf8_lossy(&out).trim().to_string();
    match is_hash(&commit) {
        true => Ok(commit),
        false => Err(format!("git rev-parse gave {commit:?}")),
    }
}

/// The newest commit of `at` (a branch, a tag, `HEAD`) at `url`, from `git
/// ls-remote`, which neither fetches nor needs the bare repository.
pub fn remote_commit(sh: &mut Shell, url: &str, at: &GitRef) -> Result<String, String> {
    let name = refspec(at);
    let peeled = format!("{name}^{{}}");
    let out = git(sh, b"/", &["ls-remote", url, &name, &peeled])?;
    let out = String::from_utf8_lossy(&out);
    let find = |want: &str| {
        out.lines().find_map(|l| match l.split_once('\t') {
            Some((commit, r)) if r == want && is_hash(commit) => Some(commit.to_string()),
            _ => None,
        })
    };
    // An annotated tag's commit is on the peeled line.
    find(&peeled)
        .or_else(|| find(&name))
        .ok_or_else(|| format!("no {name} at {url}"))
}

/// Whether `s` is a full commit hash.
pub fn is_hash(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Makes sure the files of `commit` are in [`checkout`], fetching the
/// commit if the repository doesn't have it: the commit itself (which not
/// all servers allow), else all of `at`.
pub fn extract(sh: &mut Shell, data: &[u8], url: &str, at: &GitRef, commit: &str) -> Result<(), String> {
    let dest = checkout(data, url, commit);
    if sys::is_dir(&dest) {
        return Ok(());
    }
    let dir = repository(sh, url)?;
    let object = format!("{commit}^{{commit}}");
    let have = |sh: &mut Shell| git(sh, &dir, &["cat-file", "-e", &object]).is_ok();
    if !have(sh) {
        let fetched = git(sh, &dir, &["fetch", "-q", "--no-tags", "--depth", "1", url, commit]);
        if fetched.is_err() || !have(sh) {
            git(sh, &dir, &["fetch", "-q", "--no-tags", url, &refspec(at)])?;
        }
        if !have(sh) {
            return Err(format!("{url} has no commit {commit}"));
        }
    }
    let parent = &dest[..dest.iter().rposition(|&c| c == b'/').unwrap_or(0)];
    let tmp = [
        parent,
        b"/.",
        commit.as_bytes(),
        b".",
        sys::getpid().to_string().as_bytes(),
    ]
    .concat();
    let _ = std::fs::remove_dir_all(to_path(&tmp));
    std::fs::create_dir_all(to_path(&tmp)).map_err(|e| format!("cannot create {}: {e}", show(&tmp)))?;
    let readme = [data, b"/README"].concat();
    if !to_path(&readme).exists() {
        let _ = std::fs::write(to_path(&readme), DATA_README);
    }
    let tar = [tmp.as_slice(), b"/.archive.tar"].concat();
    let r = git(
        sh,
        &dir,
        &["archive", "--format=tar", "-o", &String::from_utf8_lossy(&tar), commit],
    )
    .and_then(|_| {
        let mut script = b"command tar -x -f ".to_vec();
        script.extend(shell_quote(&tar));
        script.extend_from_slice(b" -C ");
        script.extend(shell_quote(&tmp));
        match super::capture(sh, &script) {
            Ok((0, _)) => Ok(()),
            _ => Err(format!("cannot extract {commit} of {url}")),
        }
    })
    .and_then(|_| {
        let _ = std::fs::remove_file(to_path(&tar));
        // Another shell may have got there first, which is as good.
        match std::fs::rename(to_path(&tmp), to_path(&dest)) {
            Err(_) if sys::is_dir(&dest) => Ok(()),
            r => r.map_err(|e| format!("cannot create {}: {e}", show(&dest))),
        }
    });
    if r.is_err() {
        let _ = std::fs::remove_dir_all(to_path(&tmp));
    }
    r
}

fn show(p: &[u8]) -> String {
    String::from_utf8_lossy(p).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{is_hash, repo_key};

    #[test]
    fn keys() {
        let k = repo_key("https://github.com/luispedro/luish.git");
        assert!(k.starts_with("luish-") && k.len() == "luish-".len() + 16, "{k}");
        assert_ne!(k, repo_key("https://github.com/other/luish.git"));
        assert!(repo_key("file:///tmp/x/").starts_with("x-"));
        assert!(repo_key("git@host:a/b.git").starts_with("b-"));
        assert!(repo_key("/").starts_with("repo-"));
        assert!(repo_key("file:///tmp/..").starts_with("repo-"));
    }

    #[test]
    fn hashes() {
        assert!(is_hash("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_hash("0123456789ABCDEF0123456789abcdef01234567"));
        assert!(!is_hash("abc"));
    }
}
