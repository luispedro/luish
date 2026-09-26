//! `cd` and `pwd`, and changing directory for `pushd` and `popd`.

use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

/// Lexically canonicalizes an absolute path: removes `.` components and
/// resolves `..` against the preceding component.
pub fn canonicalize(path: &[u8]) -> Vec<u8> {
    let mut comps: Vec<&[u8]> = Vec::new();
    for c in path.split(|&c| c == b'/') {
        match c {
            b"" | b"." => {}
            b".." => {
                comps.pop();
            }
            _ => comps.push(c),
        }
    }
    let mut out = Vec::new();
    for c in comps {
        out.push(b'/');
        out.extend_from_slice(c);
    }
    if out.is_empty() {
        out.push(b'/');
    }
    out
}

/// `cd` (also `chdir`, as in dash).
pub fn cd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match super::options(sh, argv, b"LPe") {
        Ok(r) => r,
        Err(s) => return Ok(s),
    };
    // The last of `-L` and `-P` wins. `-e` (POSIX 2024) makes `cd -P` fail
    // with status 1 if the new directory's name can't be found.
    let physical = opts.iter().rfind(|&&c| c != b'e') == Some(&b'P');
    let check = opts.contains(&b'e');
    let mut print = Print::Cdpath;
    let dir = match args.first() {
        None => match sh.get_var(b"HOME") {
            Some(h) if !h.is_empty() => h,
            _ => return Ok(0),
        },
        Some(d) if d == b"-" => {
            print = Print::Always;
            sh.get_var(b"OLDPWD").unwrap_or_default()
        }
        Some(d) => d.clone(),
    };
    match change_dir(sh, &argv[0], dir, physical, check, print)? {
        Some((old, status)) => {
            let new = sh.curdir.clone().unwrap_or_default();
            crate::plugins::chpwd(sh, &old, &new)?;
            Ok(status)
        }
        None => Ok(2),
    }
}

/// When [`change_dir`] prints the new directory.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Print {
    Never,
    /// If it was found through `CDPATH`.
    Cdpath,
    Always,
}

/// Changes to `dir` as `cd` does (with `CDPATH`), and sets `PWD` and
/// `OLDPWD`. Returns the old directory and the status (1 for `-e` when the
/// new directory's name can't be found), or `None` after an error message
/// if the directory can't be changed. The caller runs the `chpwd` hook.
pub fn change_dir(
    sh: &mut Shell,
    name: &[u8],
    dir: Vec<u8>,
    physical: bool,
    check: bool,
    print: Print,
) -> Result<Option<(Vec<u8>, i32)>, Flow> {
    // As in dash, an empty directory (such as an unset OLDPWD) is `.`.
    let dir = if dir.is_empty() { b".".to_vec() } else { dir };
    // CDPATH search for relative paths not starting with . or ..
    let mut candidates = Vec::new();
    let relative = dir.first() != Some(&b'/');
    let dotted = dir == b"." || dir == b".." || dir.starts_with(b"./") || dir.starts_with(b"../");
    if relative
        && !dotted
        && let Some(cdpath) = sh.get_var(b"CDPATH")
    {
        for p in cdpath.split(|&c| c == b':') {
            if p.is_empty() {
                candidates.push((dir.clone(), false));
            } else {
                let mut c = p.to_vec();
                if !c.ends_with(b"/") {
                    c.push(b'/');
                }
                c.extend_from_slice(&dir);
                candidates.push((c, true));
            }
        }
    }
    candidates.push((dir.clone(), false));
    let old = sh.curdir.clone();
    for (cand, from_cdpath) in candidates {
        let logical = |cand: &[u8]| {
            if cand.first() == Some(&b'/') {
                canonicalize(cand)
            } else {
                let mut t = old.clone().unwrap_or_else(|| b"/".to_vec());
                t.push(b'/');
                t.extend_from_slice(cand);
                canonicalize(&t)
            }
        };
        let target = if physical { cand.clone() } else { logical(&cand) };
        if sys::chdir(&target).is_err() {
            continue;
        }
        let mut status = 0;
        let new = if physical {
            // If the directory's name can't be found (it was removed),
            // `PWD` is the logical path.
            sys::getcwd().unwrap_or_else(|| {
                if check {
                    sh.berr(name, "getcwd() failed");
                    status = 1;
                }
                logical(&cand)
            })
        } else {
            target
        };
        // As in dash's `setpwd`: both are exported.
        let old = old.unwrap_or_default();
        sh.set_var(b"OLDPWD", old.clone())?;
        sh.vars.entry(b"OLDPWD").exported = true;
        sh.set_var(b"PWD", new.clone())?;
        sh.vars.entry(b"PWD").exported = true;
        sh.curdir = Some(new.clone());
        // Commands found in relative `PATH` directories must be looked up
        // again (dash's `rehash`).
        sh.hash.retain(|_, (p, _)| p.first() == Some(&b'/'));
        if print == Print::Always || (print == Print::Cdpath && from_cdpath) {
            let mut line = new;
            line.push(b'\n');
            sh.out(&line);
        }
        return Ok(Some((old, status)));
    }
    sh.berr(name, format!("can't cd to {}", String::from_utf8_lossy(&dir)));
    Ok(None)
}

pub fn pwd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let physical = argv[1..].iter().any(|a| a == b"-P");
    let dir = if physical {
        sys::getcwd()
    } else {
        sh.curdir.clone().or_else(sys::getcwd)
    };
    match dir {
        Some(mut d) => {
            d.push(b'\n');
            Ok(sh.out_status(&d))
        }
        None => {
            sh.berr(&argv[0], "getcwd() failed");
            Ok(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::canonicalize;

    #[test]
    fn canon() {
        assert_eq!(canonicalize(b"/a/./b/../c/"), b"/a/c");
        assert_eq!(canonicalize(b"/.."), b"/");
        assert_eq!(canonicalize(b"//x"), b"/x");
    }
}
