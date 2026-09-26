//! The `vcs` module: information about the version-control repository
//! around a directory, for prompts, as zsh's `vcs_info` gives. Only git
//! for now.
//!
//! `vcs::info` reads the repository's files (`HEAD`, refs, the files of an
//! operation in progress) without forking, so that it is cheap enough to
//! call at every prompt. `vcs::status` runs `git status`, for what needs
//! the index and the work tree.

use rhai::{Dynamic, Map, Module};

use super::bytes::to_str;
use super::fs::{join, parent, read, start_dir};
use super::rhai::{RhaiResult, error, with_shell};
use crate::builtins::cd::canonicalize;
use crate::sys;

/// A git repository, found from a directory in its work tree.
struct Repo {
    /// The top of the work tree.
    root: Vec<u8>,
    /// The git directory (`.git`, or the one a `.git` file names, as for a
    /// worktree or a submodule), which holds `HEAD`.
    git_dir: Vec<u8>,
    /// The directory holding the refs (differs from `git_dir` in a
    /// worktree).
    common_dir: Vec<u8>,
}

fn trim(mut b: &[u8]) -> &[u8] {
    while let [rest @ .., b'\n' | b'\r' | b' '] = b {
        b = rest;
    }
    b
}

/// A path read from a file in `base` (`.git`'s `gitdir:`, `commondir`),
/// relative to `base`.
fn relative_to(base: &[u8], p: &[u8]) -> Vec<u8> {
    if p.first() == Some(&b'/') {
        canonicalize(p)
    } else {
        canonicalize(&join(base, p))
    }
}

/// The git directory that `dir/.git` is or names, if it is one.
fn git_dir_in(dir: &[u8]) -> Option<Vec<u8>> {
    let dot_git = join(dir, b".git");
    let st = sys::stat(&dot_git)?;
    let gd = match st.st_mode & libc::S_IFMT {
        libc::S_IFDIR => dot_git,
        libc::S_IFREG => {
            let text = read(&dot_git)?;
            let target = trim(text.strip_prefix(b"gitdir:")?).trim_ascii_start();
            relative_to(dir, target)
        }
        _ => return None,
    };
    sys::stat(&join(&gd, b"HEAD")).is_some().then_some(gd)
}

/// Finds the repository whose work tree contains `dir` (absolute).
fn find(dir: &[u8]) -> Option<Repo> {
    let mut d = dir.to_vec();
    loop {
        if let Some(git_dir) = git_dir_in(&d) {
            let common_dir = match read(&join(&git_dir, b"commondir")) {
                Some(c) => relative_to(&git_dir, trim(&c)),
                None => git_dir.clone(),
            };
            return Some(Repo {
                root: d,
                git_dir,
                common_dir,
            });
        }
        d = parent(&d)?.to_vec();
    }
}

fn is_file(p: &[u8]) -> bool {
    sys::stat(p).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG)
}

impl Repo {
    fn file(&self, name: &[u8]) -> Vec<u8> {
        join(&self.git_dir, name)
    }

    fn has(&self, name: &[u8]) -> bool {
        sys::stat(&self.file(name)).is_some()
    }

    /// The first line of a file in the git directory.
    fn line(&self, name: &[u8]) -> Option<Vec<u8>> {
        let text = read(&self.file(name))?;
        let first = text.split(|&c| c == b'\n').next().unwrap_or_default();
        Some(trim(first).to_vec())
    }

    /// The commit a ref (`HEAD`, `refs/heads/main`) points to, following
    /// symbolic refs; `None` for a branch without commits.
    fn resolve(&self, name: &[u8]) -> Option<Vec<u8>> {
        let mut name = name.to_vec();
        for _ in 0..5 {
            // Refs other than those under `refs/` (`HEAD`, `MERGE_HEAD`) and
            // `refs/bisect` belong to a worktree; the others are shared.
            let dir = if name.starts_with(b"refs/") && !name.starts_with(b"refs/bisect/") {
                &self.common_dir
            } else {
                &self.git_dir
            };
            let Some(text) = read(&join(dir, &name)) else {
                return self.packed(&name);
            };
            let text = trim(&text);
            match text.strip_prefix(b"ref:") {
                Some(target) => name = target.trim_ascii_start().to_vec(),
                None => return Some(text.to_vec()),
            }
        }
        None
    }

    /// A ref in `packed-refs`.
    fn packed(&self, name: &[u8]) -> Option<Vec<u8>> {
        let text = read(&join(&self.common_dir, b"packed-refs"))?;
        text.split(|&c| c == b'\n').find_map(|line| {
            let (hash, r) = line.split_at(line.iter().position(|&c| c == b' ')?);
            (&r[1..] == name).then(|| hash.to_vec())
        })
    }

    /// The operation in progress, with the names `vcs_info` gives it, and
    /// the file holding the name of the branch being rebased.
    fn action(&self) -> (Option<&'static str>, Option<&'static [u8]>) {
        if self.has(b"rebase-apply") {
            let a = if self.has(b"rebase-apply/rebasing") {
                "rebase"
            } else if self.has(b"rebase-apply/applying") {
                "am"
            } else {
                "am/rebase"
            };
            return (Some(a), Some(b"rebase-apply/head-name"));
        }
        if self.has(b"rebase-merge") {
            let a = if self.has(b"rebase-merge/interactive") {
                "rebase-i"
            } else {
                "rebase-m"
            };
            return (Some(a), Some(b"rebase-merge/head-name"));
        }
        let a = if is_file(&self.file(b"MERGE_HEAD")) {
            "merge"
        } else if is_file(&self.file(b"BISECT_LOG")) {
            "bisect"
        } else if is_file(&self.file(b"CHERRY_PICK_HEAD")) {
            if self.has(b"sequencer") { "cherry-seq" } else { "cherry" }
        } else if is_file(&self.file(b"REVERT_HEAD")) {
            "revert"
        } else if self.has(b"sequencer") {
            "cherry-or-revert"
        } else {
            return (None, None);
        };
        (Some(a), None)
    }

    /// The step of a rebase or `git am` in progress, and the number of
    /// steps.
    fn progress(&self) -> Option<(i64, i64)> {
        let num = |name: &[u8]| -> Option<i64> { std::str::from_utf8(&self.line(name)?).ok()?.parse().ok() };
        num(b"rebase-merge/msgnum")
            .zip(num(b"rebase-merge/end"))
            .or_else(|| num(b"rebase-apply/next").zip(num(b"rebase-apply/last")))
    }

    fn stashes(&self) -> i64 {
        read(&join(&self.common_dir, b"logs/refs/stash"))
            .map_or(0, |t| t.split(|&c| c == b'\n').filter(|l| !l.is_empty()).count() as i64)
    }
}

/// Runs `git -C dir ARGS...` with standard input and standard error on
/// `/dev/null`, and returns its output if it succeeds.
fn git(dir: &[u8], args: &[&str]) -> RhaiResult<Option<Vec<u8>>> {
    with_shell(|sh| {
        let (r, w) = match sys::pipe() {
            Ok(p) => p,
            Err(e) => return error(format!("pipe: {}", sys::strerror(e))),
        };
        let argv: Vec<Vec<u8>> = ["git", "-C"]
            .iter()
            .map(|a| a.as_bytes().to_vec())
            .chain([dir.to_vec()])
            .chain(args.iter().map(|a| a.as_bytes().to_vec()))
            .collect();
        let env = sh.vars.environ();
        let Ok(pid) = sh.fork_or_error() else {
            sys::close(r);
            sys::close(w);
            return error("cannot fork");
        };
        if pid == 0 {
            sys::close(r);
            let _ = sys::dup2(w, 1);
            sys::close(w);
            if let Ok(null) = sys::open(b"/dev/null", libc::O_RDWR, 0) {
                let _ = sys::dup2(null, 0);
                let _ = sys::dup2(null, 2);
                sys::close(null);
            }
            let _ = sh.with_command_path(b"git", None, |_, path| Err::<(), i32>(sys::execve(path, &argv, &env)));
            sys::exit(127);
        }
        sys::close(w);
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = sys::read(r, &mut buf, false) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        sys::close(r);
        Ok((sh.wait_for(pid) == 0).then_some(out))
    })
}

fn str_value(b: &[u8]) -> Dynamic {
    to_str(b).into()
}

fn opt_value(b: Option<Vec<u8>>) -> Dynamic {
    b.map_or(Dynamic::UNIT, |b| str_value(&b))
}

/// `vcs::info([dir])`.
fn info(dir: Option<&str>) -> RhaiResult<Dynamic> {
    let dir = start_dir(dir)?;
    let Some(repo) = find(&dir) else {
        return Ok(Dynamic::UNIT);
    };
    let head_file = repo.line(b"HEAD").unwrap_or_default();
    let (action, head_name) = repo.action();
    let mut branch = head_file.strip_prefix(b"ref:").map(|r| r.trim_ascii_start().to_vec());
    let head = if branch.as_deref() == Some(b"refs/heads/.invalid") {
        // The reftable format, whose refs aren't files: ask git.
        branch = git(&dir, &["symbolic-ref", "-q", "HEAD"])?.map(|b| trim(&b).to_vec());
        git(&dir, &["rev-parse", "-q", "--verify", "HEAD"])?.map(|h| trim(&h).to_vec())
    } else {
        repo.resolve(b"HEAD")
    };
    // While rebasing, HEAD is detached: the branch is the one being rebased.
    if branch.is_none()
        && let Some(f) = head_name
    {
        branch = repo.line(f).filter(|b| b.starts_with(b"refs/"));
    }
    let branch = branch.map(|b| b.strip_prefix(b"refs/heads/").map_or(b.clone(), <[u8]>::to_vec));

    let mut m = Map::new();
    m.insert("vcs".into(), "git".into());
    m.insert("root".into(), str_value(&repo.root));
    let name = repo.root.rsplit(|&c| c == b'/').next().unwrap_or_default();
    m.insert("name".into(), str_value(if name.is_empty() { b"/" } else { name }));
    let sub = match dir.strip_prefix(repo.root.as_slice()) {
        Some(s) => s.strip_prefix(b"/").unwrap_or(s),
        None => b"",
    };
    m.insert("subdir".into(), str_value(if sub.is_empty() { b"." } else { sub }));
    m.insert("git_dir".into(), str_value(&repo.git_dir));
    m.insert("branch".into(), opt_value(branch));
    m.insert("head".into(), opt_value(head));
    m.insert("action".into(), action.map_or(Dynamic::UNIT, Dynamic::from));
    let (step, steps) = match (action, repo.progress()) {
        (Some(_), Some((a, b))) => (a.into(), b.into()),
        _ => (Dynamic::UNIT, Dynamic::UNIT),
    };
    m.insert("step".into(), step);
    m.insert("steps".into(), steps);
    m.insert("stashes".into(), repo.stashes().into());
    Ok(m.into())
}

/// Counts from `git status --porcelain=v2 --branch -z`.
#[derive(Default, Debug, PartialEq)]
struct Status {
    upstream: Option<Vec<u8>>,
    ahead: i64,
    behind: i64,
    staged: i64,
    unstaged: i64,
    untracked: i64,
    conflicts: i64,
}

fn parse_status(out: &[u8]) -> Status {
    let mut s = Status::default();
    let mut records = out.split(|&c| c == 0);
    while let Some(rec) = records.next() {
        let num = |b: &[u8]| -> i64 { std::str::from_utf8(b).ok().and_then(|n| n.parse().ok()).unwrap_or(0) };
        if let Some(u) = rec.strip_prefix(b"# branch.upstream ") {
            s.upstream = Some(u.to_vec());
        } else if let Some(ab) = rec.strip_prefix(b"# branch.ab ") {
            let mut it = ab.split(|&c| c == b' ');
            s.ahead = it.next().and_then(|a| a.strip_prefix(b"+")).map_or(0, num);
            s.behind = it.next().and_then(|b| b.strip_prefix(b"-")).map_or(0, num);
        } else if let [kind @ (b'1' | b'2'), b' ', x, y, ..] = rec {
            s.staged += (*x != b'.') as i64;
            s.unstaged += (*y != b'.') as i64;
            if *kind == b'2' {
                // The original path of a rename or copy.
                records.next();
            }
        } else if rec.starts_with(b"u ") {
            s.conflicts += 1;
        } else if rec.starts_with(b"? ") {
            s.untracked += 1;
        }
    }
    s
}

/// `vcs::status([dir])`.
fn status(dir: Option<&str>) -> RhaiResult<Dynamic> {
    let dir = start_dir(dir)?;
    if find(&dir).is_none() {
        return Ok(Dynamic::UNIT);
    }
    let args = ["--no-optional-locks", "status", "--porcelain=v2", "--branch", "-z"];
    let Some(out) = git(&dir, &args)? else {
        return Ok(Dynamic::UNIT);
    };
    let s = parse_status(&out);
    let mut m = Map::new();
    let clean = s.staged + s.unstaged + s.untracked + s.conflicts == 0;
    m.insert("upstream".into(), opt_value(s.upstream));
    m.insert("ahead".into(), s.ahead.into());
    m.insert("behind".into(), s.behind.into());
    m.insert("staged".into(), s.staged.into());
    m.insert("unstaged".into(), s.unstaged.into());
    m.insert("untracked".into(), s.untracked.into());
    m.insert("conflicts".into(), s.conflicts.into());
    m.insert("clean".into(), clean.into());
    Ok(m.into())
}

pub fn module() -> Module {
    let mut m = Module::new();
    m.set_native_fn("info", || info(None));
    m.set_native_fn("info", |dir: &str| info(Some(dir)));
    m.set_native_fn("status", || status(None));
    m.set_native_fn("status", |dir: &str| status(Some(dir)));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain() {
        let out = b"# branch.oid 1234\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\0\
1 M. N... 100644 100644 100644 a b f1\0\
1 .M N... 100644 100644 100644 a b f2\0\
1 MM N... 100644 100644 100644 a b f3\0\
2 R. N... 100644 100644 100644 a b R100 new\0old\0\
u UU N... 100644 100644 100644 100644 a b c conflict\0\
? new file\0? 1 M. looks like an entry\0";
        assert_eq!(
            parse_status(out),
            Status {
                upstream: Some(b"origin/main".to_vec()),
                ahead: 2,
                behind: 3,
                staged: 3,
                unstaged: 2,
                untracked: 2,
                conflicts: 1,
            }
        );
        assert_eq!(
            parse_status(b"# branch.oid (initial)\0# branch.head main\0"),
            Status::default()
        );
    }
}
