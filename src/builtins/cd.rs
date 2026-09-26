//! `cd` and `pwd`.

use crate::shell::{ExecResult, Shell};
use crate::sys;

/// Lexically canonicalizes an absolute path: removes `.` components and
/// resolves `..` against the preceding component.
fn canonicalize(path: &[u8]) -> Vec<u8> {
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

impl Shell {
    /// The logical current directory: `$PWD` if it is valid.
    pub fn logical_pwd(&self) -> Option<Vec<u8>> {
        let pwd = self.get_var(b"PWD")?;
        (pwd.first() == Some(&b'/') && sys::same_file(&pwd, b".")).then_some(pwd)
    }
}

pub fn cd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut physical = false;
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        match a.as_slice() {
            b"-L" => physical = false,
            b"-P" => physical = true,
            b"--" => {
                i += 1;
                break;
            }
            _ => break,
        }
        i += 1;
    }
    let mut print = false;
    let dir = match argv.get(i) {
        None => match sh.get_var(b"HOME") {
            Some(h) if !h.is_empty() => h,
            _ => return Ok(0),
        },
        Some(d) if d == b"-" => {
            print = true;
            match sh.get_var(b"OLDPWD") {
                Some(d) => d,
                None => {
                    sh.berr(&argv[0], "OLDPWD not set");
                    return Ok(2);
                }
            }
        }
        Some(d) => d.clone(),
    };
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
    let old = sh.logical_pwd().or_else(sys::getcwd);
    for (cand, from_cdpath) in candidates {
        let target = if physical {
            cand.clone()
        } else if cand.first() == Some(&b'/') {
            canonicalize(&cand)
        } else {
            let mut t = old.clone().unwrap_or_else(|| b"/".to_vec());
            t.push(b'/');
            t.extend_from_slice(&cand);
            canonicalize(&t)
        };
        if sys::chdir(&target).is_err() {
            continue;
        }
        let new = if physical {
            sys::getcwd().unwrap_or(target)
        } else {
            target
        };
        if let Some(o) = old {
            sh.set_var(b"OLDPWD", o)?;
        }
        sh.set_var(b"PWD", new.clone())?;
        if print || from_cdpath {
            let mut line = new;
            line.push(b'\n');
            sh.out(&line);
        }
        return Ok(0);
    }
    sh.berr(&argv[0], format!("can't cd to {}", String::from_utf8_lossy(&dir)));
    Ok(2)
}

pub fn pwd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let physical = argv[1..].iter().any(|a| a == b"-P");
    let dir = if physical {
        sys::getcwd()
    } else {
        sh.logical_pwd().or_else(sys::getcwd)
    };
    match dir {
        Some(mut d) => {
            d.push(b'\n');
            Ok(sh.out_or_err(&argv[0], &d))
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
