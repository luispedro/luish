//! The directory stack, as in zsh: `pushd`, `popd` and `dirs`.
//!
//! The stack ([`Shell::dirstack`]) doesn't hold the current directory, but
//! the commands number it as entry 0, followed by the stack, most recent
//! first. `+n` counts entries from the start of that list, `-n` from its
//! end. zsh's options for the stack (`AUTO_PUSHD`, `PUSHD_MINUS`, ...) are
//! all off. zsh's sh emulation sets `POSIX_CD`, which disables `+n` and
//! `-n`; luish always has them, as zsh does by default.

use super::cd::{Print, change_dir};
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

/// `pushd [-qLP] [dir | +n | -n]`.
pub fn pushd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let Some((quiet, physical, arg)) = stack_args(sh, argv) else {
        return Ok(1);
    };
    let old = match arg {
        // As in zsh, without a directory `pushd` swaps the top two entries,
        // or goes to `HOME` if the stack is empty.
        None if sh.dirstack.is_empty() => {
            let home = sh.get_var(b"HOME").unwrap_or_default();
            push(sh, &argv[0], home, physical)?
        }
        None => {
            let top = sh.dirstack.remove(0);
            let old = push(sh, &argv[0], top.clone(), physical)?;
            if old.is_none() {
                sh.dirstack.insert(0, top);
            }
            old
        }
        Some(a) => match entry(sh, &argv[0], a) {
            Some(Some(n)) => rotate(sh, &argv[0], n, physical)?,
            Some(None) => return Ok(1),
            None if a == b"-" => {
                let dir = sh.get_var(b"OLDPWD").unwrap_or_default();
                push(sh, &argv[0], dir, physical)?
            }
            None => push(sh, &argv[0], a.clone(), physical)?,
        },
    };
    match old {
        Some(old) => changed(sh, &old, quiet),
        None => Ok(1),
    }
}

/// `popd [-qLP] [+n | -n]`.
pub fn popd(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let Some((quiet, physical, arg)) = stack_args(sh, argv) else {
        return Ok(1);
    };
    let n = match arg {
        None => 0,
        Some(a) => match entry(sh, &argv[0], a) {
            Some(Some(n)) => n,
            Some(None) => return Ok(1),
            // zsh does something odd with a directory here (usually
            // nothing, with status 0).
            None => {
                sh.berr(&argv[0], format!("bad argument: {}", String::from_utf8_lossy(a)));
                return Ok(1);
            }
        },
    };
    if n > 0 {
        sh.dirstack.remove(n - 1);
        if sh.opt(Opt::Interactive) && !quiet {
            print_stack(sh, false, Format::Line);
        }
        return Ok(0);
    }
    let dir = if !sh.dirstack.is_empty() {
        // As in zsh, the entry is removed even if it can't be changed to,
        // so that a directory that was removed doesn't stay on the stack.
        sh.dirstack.remove(0)
    } else if arg.is_some() {
        // As in zsh, `popd +0` with an empty stack stays where it is.
        current(sh)
    } else {
        sh.berr(&argv[0], "directory stack empty");
        return Ok(1);
    };
    match change_dir(sh, &argv[0], dir, physical, false, Print::Never)? {
        Some((old, _)) => changed(sh, &old, quiet),
        None => Ok(1),
    }
}

/// `dirs [-clpv] [dir ...]`.
pub fn dirs(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    // The status for a bad option is zsh's.
    let Ok((opts, args)) = super::options(sh, argv, b"clpv") else {
        return Ok(1);
    };
    if opts.contains(&b'c') {
        sh.dirstack.clear();
    } else if !args.is_empty() {
        sh.dirstack = args.to_vec();
    } else {
        let format = if opts.contains(&b'v') {
            Format::Numbered
        } else if opts.contains(&b'p') {
            Format::Lines
        } else {
            Format::Line
        };
        print_stack(sh, opts.contains(&b'l'), format);
    }
    Ok(0)
}

/// Parses the arguments of `pushd` and `popd`: whether `-q` and `-P` were
/// given (as in zsh, `-P` wins over `-L`), and the operand. As in zsh, an
/// argument that isn't made of these letters (such as `-1` or `-`) is the
/// operand. More than one operand is an error.
fn stack_args<'a>(sh: &Shell, argv: &'a [Vec<u8>]) -> Option<(bool, bool, Option<&'a Vec<u8>>)> {
    let (mut quiet, mut physical) = (false, false);
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a == b"--" {
            i += 1;
            break;
        }
        if a.len() < 2 || a[0] != b'-' || !a[1..].iter().all(|c| b"qLP".contains(c)) {
            break;
        }
        i += 1;
        quiet |= a.contains(&b'q');
        physical |= a.contains(&b'P');
    }
    match &argv[i..] {
        [] => Some((quiet, physical, None)),
        [a] => Some((quiet, physical, Some(a))),
        _ => {
            sh.berr(&argv[0], "too many arguments");
            None
        }
    }
}

/// The entry that `+n` or `-n` names, with the current directory as entry
/// 0. `None` if `arg` isn't of this form; `Some(None)` after an error
/// message if there is no such entry.
fn entry(sh: &Shell, name: &[u8], arg: &[u8]) -> Option<Option<usize>> {
    let (&sign, digits) = arg.split_first()?;
    if !matches!(sign, b'+' | b'-') || digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let len = sh.dirstack.len() + 1;
    // Too many digits for a number is out of range too.
    let n = std::str::from_utf8(digits).ok()?.parse::<usize>().unwrap_or(usize::MAX);
    let n = if sign == b'+' {
        Some(n)
    } else {
        (len - 1).checked_sub(n)
    };
    match n.filter(|&n| n < len) {
        Some(n) => Some(Some(n)),
        None => {
            sh.berr(name, "no such entry in dir stack");
            Some(None)
        }
    }
}

/// Changes to `dir` and pushes the previous directory. Returns the previous
/// directory, or `None` after an error message.
fn push(sh: &mut Shell, name: &[u8], dir: Vec<u8>, physical: bool) -> Result<Option<Vec<u8>>, Flow> {
    let Some((old, _)) = change_dir(sh, name, dir, physical, false, Print::Never)? else {
        return Ok(None);
    };
    if !old.is_empty() {
        sh.dirstack.insert(0, old.clone());
    }
    Ok(Some(old))
}

/// Changes to entry `n` and rotates the stack so that the entries before
/// it go to the end. Returns the previous directory, or `None` after an
/// error message (the stack is then unchanged).
fn rotate(sh: &mut Shell, name: &[u8], n: usize, physical: bool) -> Result<Option<Vec<u8>>, Flow> {
    let mut all = vec![current(sh)];
    all.extend(sh.dirstack.iter().cloned());
    let Some((old, _)) = change_dir(sh, name, all[n].clone(), physical, false, Print::Never)? else {
        return Ok(None);
    };
    all.rotate_left(n);
    all.remove(0);
    sh.dirstack = all;
    Ok(Some(old))
}

/// After `pushd` or `popd` changed the directory: as in zsh, an interactive
/// shell prints the stack (unless `-q` was given), then the `chpwd` hook
/// runs.
fn changed(sh: &mut Shell, old: &[u8], quiet: bool) -> ExecResult {
    if sh.opt(Opt::Interactive) && !quiet {
        print_stack(sh, false, Format::Line);
    }
    let new = sh.curdir.clone().unwrap_or_default();
    crate::plugins::chpwd(sh, old, &new)?;
    Ok(0)
}

fn current(sh: &Shell) -> Vec<u8> {
    sh.curdir.clone().or_else(sys::getcwd).unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    /// On one line, separated by spaces.
    Line,
    /// One per line (`-p`).
    Lines,
    /// One per line, after its number and a tab (`-v`).
    Numbered,
}

/// Prints the current directory and the stack, with `HOME` shown as `~`
/// unless `full`.
fn print_stack(sh: &Shell, full: bool, format: Format) {
    let home = if full { None } else { sh.get_var(b"HOME") };
    let mut out = Vec::new();
    let dirs = std::iter::once(current(sh)).chain(sh.dirstack.iter().cloned());
    for (i, dir) in dirs.enumerate() {
        match format {
            Format::Line if i > 0 => out.push(b' '),
            Format::Numbered => out.extend(format!("{i}\t").bytes()),
            _ => {}
        }
        out.extend(abbreviate(home.as_deref(), &dir));
        if format != Format::Line {
            out.push(b'\n');
        }
    }
    if format == Format::Line {
        out.push(b'\n');
    }
    sh.out(&out);
}

/// `dir` with `home` replaced by `~` at its start. As in zsh, a `HOME` of
/// `/` is not replaced.
fn abbreviate(home: Option<&[u8]>, dir: &[u8]) -> Vec<u8> {
    if let Some(h) = home
        && h.len() > 1
        && let Some(rest) = dir.strip_prefix(h)
        && (rest.is_empty() || rest[0] == b'/')
    {
        return [b"~", rest].concat();
    }
    dir.to_vec()
}

#[cfg(test)]
mod tests {
    use super::abbreviate;

    #[test]
    fn home() {
        assert_eq!(abbreviate(Some(b"/h"), b"/h"), b"~");
        assert_eq!(abbreviate(Some(b"/h"), b"/h/x"), b"~/x");
        assert_eq!(abbreviate(Some(b"/h"), b"/hx"), b"/hx");
        assert_eq!(abbreviate(Some(b"/"), b"/x"), b"/x");
        assert_eq!(abbreviate(None, b"/h"), b"/h");
    }
}
