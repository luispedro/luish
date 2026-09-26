//! `fc`: list, or edit and re-run, commands from the history.
//!
//! This follows POSIX and upstream dash (`histcmd` in `histedit.c`; Debian
//! builds dash without it). As in bash, the entry of the `fc` command is
//! left out of the history it works on, and when `fc` re-runs commands it
//! replaces its own entry with them and echoes them to stderr.

use std::cell::Cell;
use std::os::unix::ffi::OsStrExt;

use super::vars::single_quote;
use crate::interactive;
use crate::interactive::history::ShellHistory;
use crate::shell::{ExecResult, Shell};
use crate::sys;

/// How deeply `fc` may re-run itself (dash's `MAXHISTLOOPS`).
const MAX_DEPTH: u32 = 4;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Counts the `fc` commands re-running commands, while it lives.
struct DepthGuard;

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// An optionally signed decimal number (as opposed to a command prefix).
fn is_number(s: &[u8]) -> bool {
    let d = s.strip_prefix(b"-").or_else(|| s.strip_prefix(b"+")).unwrap_or(s);
    !d.is_empty() && d.iter().all(u8::is_ascii_digit)
}

/// The event number that `first` or `last` names: a negative number counts
/// back from the newest entry, other numbers are event numbers (brought
/// into range, as POSIX asks), and a string names the newest command that
/// starts with it.
fn event(h: &ShellHistory, (lo, hi): (usize, usize), s: &[u8]) -> Result<usize, String> {
    if is_number(s) {
        let n = std::str::from_utf8(&s[usize::from(!s[0].is_ascii_digit())..])
            .ok()
            .and_then(|d| d.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        if s[0] == b'-' {
            Ok(hi.saturating_sub(n.saturating_sub(1)).max(lo))
        } else {
            Ok(n.clamp(lo, hi))
        }
    } else {
        (lo..=hi)
            .rev()
            .find(|&n| h.event(n).is_some_and(|e| e.as_bytes().starts_with(s)))
            .ok_or_else(|| format!("history pattern not found: {}", String::from_utf8_lossy(s)))
    }
}

/// The selected entries, in the order to list or run them.
fn select(h: &ShellHistory, first: &[u8], last: &[u8], reverse: bool) -> Result<Vec<(usize, String)>, String> {
    let Some(range) = h.range() else {
        return Ok(Vec::new());
    };
    let (mut a, mut b) = (event(h, range, first)?, event(h, range, last)?);
    if reverse {
        std::mem::swap(&mut a, &mut b);
    }
    let nums: Vec<usize> = if a <= b {
        (a..=b).collect()
    } else {
        (b..=a).rev().collect()
    };
    Ok(nums
        .into_iter()
        .filter_map(|n| Some((n, h.event(n)?.to_owned())))
        .collect())
}

/// Replaces the first occurrence of `old` with `new` (`fc -s old=new`).
fn replace_first(s: &str, old: &[u8], new: &[u8]) -> Vec<u8> {
    let b = s.as_bytes();
    match (!old.is_empty())
        .then(|| b.windows(old.len()).position(|w| w == old))
        .flatten()
    {
        Some(i) => [&b[..i], new, &b[i + old.len()..]].concat(),
        None => b.to_vec(),
    }
}

pub fn fc(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let name = &argv[0];
    let fail = |msg: &str| -> ExecResult {
        sh.berr(name, msg);
        Ok(2)
    };
    let mut editor: Option<Vec<u8>> = None;
    let (mut list, mut numbers, mut reverse, mut rerun) = (false, true, false, false);
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a.len() < 2 || a[0] != b'-' || is_number(a) {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        for (j, &c) in a.iter().enumerate().skip(1) {
            match c {
                b'e' => {
                    if j + 1 < a.len() {
                        editor = Some(a[j + 1..].to_vec());
                    } else if let Some(e) = argv.get(i) {
                        editor = Some(e.clone());
                        i += 1;
                    } else {
                        return fail("option -e expects argument");
                    }
                    break;
                }
                b'l' => list = true,
                b'n' => numbers = false,
                b'r' => reverse = true,
                b's' => rerun = true,
                _ => return fail(&format!("unknown option: -{}", c as char)),
            }
        }
    }
    let mut args = &argv[i..];
    if interactive::with_history(|_| ()).is_none() {
        return fail("history not active");
    }

    if rerun {
        editor = None;
        list = false;
    } else if editor.is_some() || !list {
        list = false;
        let var = |name: &[u8]| sh.get_var(name).filter(|v| !v.is_empty());
        let e = editor
            .take()
            .or_else(|| var(b"FCEDIT"))
            .or_else(|| var(b"EDITOR"))
            .unwrap_or_else(|| b"ed".to_vec());
        // `-e -` re-runs without editing, like `-s`.
        if e != b"-" {
            editor = Some(e);
        }
    }
    let mut subst = None;
    if !list
        && let Some(a) = args.first()
        && let Some(eq) = a.iter().position(|&c| c == b'=')
    {
        subst = Some((&a[..eq], &a[eq + 1..]));
        args = &args[1..];
    }
    let (first, last): (&[u8], &[u8]) = match args {
        [] => (if list { b"-16" } else { b"-1" }, b"-1"),
        [f] => (f, if list { b"-1" } else { f }),
        [f, l] => (f, l),
        _ => return fail("too many args"),
    };
    let selected = match interactive::with_history(|h| select(h, first, last, reverse)) {
        Some(Ok(s)) => s,
        Some(Err(msg)) => return fail(&msg),
        None => return fail("history not active"),
    };

    if list {
        let mut out = Vec::new();
        for (n, text) in &selected {
            for (k, line) in text.split('\n').enumerate() {
                if k == 0 && numbers {
                    out.extend_from_slice(n.to_string().as_bytes());
                }
                out.push(b'\t');
                out.extend_from_slice(line.as_bytes());
                out.push(b'\n');
            }
        }
        return Ok(sh.out_status(&out));
    }
    if selected.is_empty() {
        return fail("no command found");
    }

    if DEPTH.with(|d| d.get()) >= MAX_DEPTH {
        return fail("called recursively too many times");
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    let _guard = DepthGuard;

    let commands: Vec<Vec<u8>> = match editor {
        None => selected
            .iter()
            .map(|(_, text)| match subst {
                Some((old, new)) => replace_first(text, old, new),
                None => text.as_bytes().to_vec(),
            })
            .collect(),
        Some(editor) => {
            let (fd, path) = sh.temp_file(b"luish-fc-")?;
            let mut text = Vec::new();
            for (_, t) in &selected {
                text.extend_from_slice(t.as_bytes());
                text.push(b'\n');
            }
            let written = sys::write_all(fd, &text);
            sys::close(fd);
            if !written {
                sys::unlink(&path);
                return fail("cannot write temp file");
            }
            let mut cmd = editor;
            cmd.push(b' ');
            cmd.extend_from_slice(&single_quote(&path));
            let r = sh.run_string(&cmd);
            let edited = std::fs::read(std::ffi::OsStr::from_bytes(&path));
            sys::unlink(&path);
            // POSIX: an editor that fails suppresses the re-execution.
            match r? {
                0 => {}
                status => return Ok(status),
            }
            match edited {
                Ok(t) if t.iter().any(|c| !c.is_ascii_whitespace()) => vec![t],
                _ => return Ok(0),
            }
        }
    };

    interactive::with_history(ShellHistory::remove_current);
    let mut status = 0;
    for cmd in commands {
        let mut echo = cmd.clone();
        if echo.last() != Some(&b'\n') {
            echo.push(b'\n');
        }
        sys::write_all(2, &echo);
        interactive::add_history(sh, &cmd);
        status = sh.run_string(&cmd)?;
    }
    Ok(status)
}
