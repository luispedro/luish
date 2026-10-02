//! `.`, `source`, `times`, `alias`, `unalias`, `command`, `type`, `hash`, `ulimit`,
//! `umask`.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;

use super::vars::single_quote;
use crate::exec::CommandKind;
use crate::expand::pattern::Pattern;
use crate::expand::split::XChar;
use crate::frames::{Frame, FrameKind};
use crate::lexer::AliasKind;
use crate::parser::is_reserved;
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

pub fn dot(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run_file(sh, argv, false)
}

/// `source`, as in zsh: `.`, but a name without `/` is looked for in the
/// current directory before `PATH`, and further arguments are the positional
/// parameters while the file runs.
pub fn source(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() <= 2 {
        return run_file(sh, argv, true);
    }
    let saved_pos = std::mem::replace(&mut sh.positional, argv[2..].to_vec());
    let saved_getopts = (sh.optind, sh.optoff);
    sh.reset_getopts();
    let r = run_file(sh, &argv[..2], true);
    sh.positional = saved_pos;
    (sh.optind, sh.optoff) = saved_getopts;
    r
}

fn is_regular(p: &[u8]) -> bool {
    sys::stat(p).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG)
}

fn run_file(sh: &mut Shell, argv: &[Vec<u8>], cwd_first: bool) -> ExecResult {
    let Some(name) = argv.get(1) else {
        sh.berr(&argv[0], "filename argument required");
        return Err(Flow::Error(2));
    };
    let path = if name.contains(&b'/') || (cwd_first && is_regular(name)) {
        Some(name.clone())
    } else {
        let path = sh.get_var(b"PATH").unwrap_or_default();
        path.split(|&c| c == b':').find_map(|dir| {
            let mut p = if dir.is_empty() { b".".to_vec() } else { dir.to_vec() };
            p.push(b'/');
            p.extend_from_slice(name);
            is_regular(&p).then_some(p)
        })
    };
    if let (Some(rec), Some(p)) = (&mut sh.sourced_files, &path) {
        rec.push(match &sh.curdir {
            Some(dir) if p.first() != Some(&b'/') => [dir.as_slice(), b"/", p].concat(),
            _ => p.clone(),
        });
    }
    let text = match path.as_ref().map(|p| std::fs::read(OsStr::from_bytes(p))) {
        Some(Ok(t)) => t,
        // As in dash, a directory reads as empty.
        Some(Err(e)) if e.raw_os_error() == Some(libc::EISDIR) => Vec::new(),
        r => {
            let e = r
                .and_then(|r| r.err())
                .and_then(|e| e.raw_os_error())
                .unwrap_or(libc::ENOENT);
            sh.berr(
                &argv[0],
                format!(
                    "cannot open {}: {}",
                    String::from_utf8_lossy(name),
                    crate::exec::redirect::open_error(e)
                ),
            );
            return Err(Flow::Error(2));
        }
    };
    let saved_lineno = sh.lineno;
    sh.lineno = 1;
    sh.dot_depth += 1;
    sh.frames.push(Frame {
        kind: FrameKind::Source,
        file: path.map(Rc::from),
        lines_in_file: true,
        call_line: saved_lineno,
    });
    let r = sh.run_string(&text);
    sh.frames.pop();
    sh.dot_depth -= 1;
    sh.lineno = saved_lineno;
    match r {
        Err(Flow::Return(n)) => Ok(n),
        r => r,
    }
}

/// `caller [N]` (bash): where the current function, or file read with `.`,
/// was called from (`Shell::caller`).
pub fn caller(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let args = match super::options(sh, argv, b"") {
        Ok((_, args)) => args,
        Err(s) => return Ok(s),
    };
    let n = match args {
        [] => None,
        [n] => match super::parse_uint(n) {
            Some(n) if n >= 0 => Some(n as usize),
            _ => {
                sh.berr(&argv[0], format!("{}: invalid number", String::from_utf8_lossy(n)));
                return Ok(2);
            }
        },
        _ => {
            sh.berr(&argv[0], "too many arguments");
            return Ok(2);
        }
    };
    Ok(match sh.caller(n) {
        Some(out) => sh.out_status(&out),
        None => 1,
    })
}

pub fn times(sh: &mut Shell, _argv: &[Vec<u8>]) -> ExecResult {
    // SAFETY: plain times(2) and sysconf.
    let (t, hz) = unsafe {
        let mut t: libc::tms = std::mem::zeroed();
        libc::times(&mut t);
        (t, libc::sysconf(libc::_SC_CLK_TCK) as f64)
    };
    let f = |ticks: libc::clock_t| {
        let secs = ticks as f64 / hz;
        let m = (secs / 60.0).floor();
        format!("{}m{:.6}s", m as i64, secs - m * 60.0)
    };
    let out = format!(
        "{} {}\n{} {}\n",
        f(t.tms_utime),
        f(t.tms_stime),
        f(t.tms_cutime),
        f(t.tms_cstime)
    );
    Ok(sh.out_status(out.as_bytes()))
}

fn alias_line(name: &[u8], value: &[u8]) -> Vec<u8> {
    let mut l = name.to_vec();
    l.push(b'=');
    l.extend(single_quote(value));
    l.push(b'\n');
    l
}

/// The command that defines an alias (`alias -L`, `savestate`).
pub fn alias_command(name: &[u8], value: &[u8], kind: AliasKind) -> Vec<u8> {
    let mut l = b"alias ".to_vec();
    match kind {
        AliasKind::Regular => {}
        AliasKind::Global => l.extend_from_slice(b"-g "),
        AliasKind::Suffix => l.extend_from_slice(b"-s "),
    }
    if name.first().is_some_and(|&c| c == b'-' || c == b'+') {
        l.extend_from_slice(b"-- ");
    }
    let plain = |c: &u8| c.is_ascii_alphanumeric() || b"_-.,+/:@%^!".contains(c);
    if !name.is_empty() && name.iter().all(plain) {
        l.extend_from_slice(name);
    } else {
        l.extend(single_quote(name));
    }
    l.push(b'=');
    l.extend(single_quote(value));
    l.push(b'\n');
    l
}

/// A pattern for `alias -m`, `unalias -m` and `print -m`, where a backslash quotes the
/// next character.
pub(super) fn name_pattern(p: &[u8]) -> Pattern {
    let mut x = Vec::with_capacity(p.len());
    let mut it = p.iter();
    while let Some(&b) = it.next() {
        x.push(match (b, it.clone().next()) {
            (b'\\', Some(&n)) => {
                it.next();
                XChar { b: n, quoted: true }
            }
            _ => XChar { b, quoted: false },
        });
    }
    Pattern::new(&x)
}

/// `alias [{+|-}gmrsL] [name[=value]...]`, with zsh's options: `-g` and
/// `-s` define global and suffix aliases; for printing, `-g`, `-r` and `-s`
/// select global, regular or suffix aliases, `-m` takes the names as
/// patterns, `-L` prints `alias` commands, and `+` prints names only.
pub fn alias(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (mut kind, mut pattern, mut commands, mut names_only) = (None, false, false, false);
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a == b"--" {
            i += 1;
            break;
        }
        let plus = a.first() == Some(&b'+');
        if !(plus || (a.len() >= 2 && a[0] == b'-')) {
            break;
        }
        names_only |= plus;
        i += 1;
        for &c in &a[1..] {
            let k = match c {
                b'g' => AliasKind::Global,
                b'r' => AliasKind::Regular,
                b's' => AliasKind::Suffix,
                b'm' => {
                    pattern = true;
                    continue;
                }
                b'L' => {
                    commands = true;
                    continue;
                }
                _ => {
                    sh.berr(&argv[0], format!("Illegal option {}{}", a[0] as char, c as char));
                    return Ok(2);
                }
            };
            if kind.is_some_and(|old| old != k) {
                sh.berr(&argv[0], "illegal combination of options");
                return Ok(2);
            }
            kind = Some(k);
        }
    }
    let args = &argv[i..];
    let suffix = kind == Some(AliasKind::Suffix);
    let entry = |name: &[u8], value: &[u8], k: AliasKind| -> Vec<u8> {
        if kind.is_some_and(|want| want != k) {
            Vec::new()
        } else if names_only {
            [name, b"\n"].concat()
        } else if commands {
            alias_command(name, value, k)
        } else {
            alias_line(name, value)
        }
    };
    let all: Vec<(&[u8], &[u8], AliasKind)> = if suffix {
        let s = sh.aliases.sorted_suffixes();
        s.into_iter().map(|(n, v)| (n, v, AliasKind::Suffix)).collect()
    } else {
        (sh.aliases.sorted().into_iter())
            .map(|(n, a)| (n, &a.value[..], a.kind()))
            .collect()
    };
    if args.is_empty() || pattern {
        let pats: Vec<_> = args.iter().map(|a| name_pattern(a)).collect();
        let out: Vec<u8> = (all.iter())
            .filter(|(n, ..)| pats.is_empty() || pats.iter().any(|p| p.matches(n)))
            .flat_map(|&(n, v, k)| entry(n, v, k))
            .collect();
        return Ok(sh.out_status(&out));
    }
    let mut out = Vec::new();
    let mut status = 0;
    let mut defs = Vec::new();
    for a in args {
        match a.iter().position(|&c| c == b'=') {
            Some(i) if i > 0 => defs.push((a[..i].to_vec(), a[i + 1..].to_vec())),
            _ => match all.iter().find(|e| e.0 == &a[..]) {
                Some(&(n, v, k)) => out.extend(entry(n, v, k)),
                None => {
                    sh.berr(&argv[0], format!("{}: not found", String::from_utf8_lossy(a)));
                    status = 1;
                }
            },
        }
    }
    sh.out(&out);
    let aliases = Rc::make_mut(&mut sh.aliases);
    for (name, value) in defs {
        match kind {
            Some(AliasKind::Suffix) => aliases.insert_suffix(name, value),
            k => aliases.insert(name, value, k == Some(AliasKind::Global)),
        }
    }
    Ok(status)
}

/// `unalias [-ams] name...`: `-s` removes suffix aliases (otherwise
/// regular and global ones), `-a` all of them, and with `-m` the names are
/// patterns.
pub fn unalias(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match super::options(sh, argv, b"ams") {
        Ok(o) => o,
        Err(status) => return Ok(status),
    };
    let suffix = opts.contains(&b's');
    let aliases = Rc::make_mut(&mut sh.aliases);
    if opts.contains(&b'a') {
        if suffix {
            aliases.clear_suffixes();
        } else {
            aliases.clear();
        }
        return Ok(0);
    }
    if opts.contains(&b'm') {
        let pats: Vec<_> = args.iter().map(|a| name_pattern(a)).collect();
        let names: Vec<Vec<u8>> = if suffix {
            aliases.sorted_suffixes().into_iter().map(|e| e.0.to_vec()).collect()
        } else {
            aliases.sorted().into_iter().map(|e| e.0.to_vec()).collect()
        };
        let mut found = false;
        for n in names.iter().filter(|n| pats.iter().any(|p| p.matches(n))) {
            found = true;
            if suffix {
                aliases.remove_suffix(n);
            } else {
                aliases.remove(n);
            }
        }
        return Ok(if found || args.is_empty() { 0 } else { 1 });
    }
    let mut status = 0;
    for a in args {
        let removed = if suffix {
            Rc::make_mut(&mut sh.aliases).remove_suffix(a)
        } else {
            Rc::make_mut(&mut sh.aliases).remove(a)
        };
        if !removed {
            sh.berr(&argv[0], format!("{}: not found", String::from_utf8_lossy(a)));
            status = 1;
        }
    }
    Ok(status)
}

/// Describes a command for `command -v` (`verbose` false) or `-V`/`type`.
/// dash's `describe_command` for `type`, `command -v` (brief) and
/// `command -V` (verbose). With `alt_path` (`command -p`), that path is
/// searched instead of `PATH`. Prints to stdout; returns the status.
fn describe(sh: &mut Shell, name: &[u8], verbose: bool, alt_path: Option<&[u8]>) -> i32 {
    let n = String::from_utf8_lossy(name);
    let line = |what: &str| {
        if verbose {
            format!("{n} {what}").into_bytes()
        } else {
            name.to_vec()
        }
    };
    let text = if is_reserved(name) {
        line("is a shell keyword")
    } else if let Some((alias, value, kind)) = (sh.aliases.get(name))
        .map(|a| (name, &a.value[..], a.kind()))
        .or_else(|| (sh.aliases.for_suffix(name)).map(|(s, v)| (s, v, AliasKind::Suffix)))
    {
        if verbose {
            let what = match kind {
                AliasKind::Regular => "an alias",
                AliasKind::Global => "a global alias",
                AliasKind::Suffix => "a suffix alias",
            };
            // As in zsh, a suffix alias is shown by its suffix.
            let alias = String::from_utf8_lossy(alias);
            format!("{alias} is {what} for {}", String::from_utf8_lossy(value)).into_bytes()
        } else {
            let mut l = alias_command(alias, value, kind);
            l.pop();
            l
        }
    } else {
        match sh.lookup_command(name, true) {
            CommandKind::Special(_) => line("is a special shell builtin"),
            CommandKind::Function(_) => line("is a shell function"),
            CommandKind::Builtin(_) => line("is a shell builtin"),
            CommandKind::Extension => match crate::plugins::builtin_plugin(sh, name) {
                Some(p) => line(&format!(
                    "is a shell builtin from plugin {}",
                    String::from_utf8_lossy(&p)
                )),
                None => line("is a shell builtin"),
            },
            CommandKind::External => {
                let tracked = alt_path.is_none() && sh.hash.contains_key(name);
                let found = if name.contains(&b'/') {
                    // As in dash, any file will do.
                    sys::stat(name).map(|_| name.to_vec())
                } else if let Some(p) = alt_path {
                    crate::path::search(p, name).map(|(f, _, _)| f)
                } else {
                    sh.find_in_path(name).map(|(f, _)| f)
                };
                let Some(path) = found else {
                    if verbose {
                        sh.out(format!("{n}: not found\n").as_bytes());
                    }
                    return 127;
                };
                if verbose {
                    let mut s = format!("{n} is {}", if tracked { "a tracked alias for " } else { "" }).into_bytes();
                    s.extend(path);
                    s
                } else {
                    path
                }
            }
        }
    };
    let mut text = text;
    text.push(b'\n');
    sh.out(&text);
    0
}

pub fn command(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut i = 1;
    let mut verbose = None;
    let mut alt_path = None;
    while let Some(a) = argv.get(i) {
        if a == b"--" {
            i += 1;
            break;
        }
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        for &c in &a[1..] {
            match c {
                b'v' => verbose = Some(verbose.unwrap_or(false)),
                b'V' => verbose = Some(true),
                b'p' => alt_path = Some(crate::path::DEFAULT_PATH),
                _ => {
                    sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                    return Ok(2);
                }
            }
        }
        i += 1;
    }
    let args = &argv[i..];
    if args.is_empty() {
        return Ok(0);
    }
    if let Some(verbose) = verbose {
        // As in dash, only the first name is described.
        return Ok(describe(sh, &args[0], verbose, alt_path));
    }
    // `command` suppresses function lookup, and errors from special
    // built-ins no longer exit the shell.
    match sh.run_argv(args, false, alt_path) {
        Err(Flow::Error(n)) => Ok(n),
        r => r,
    }
}

/// `builtin name args`, as in zsh and bash: runs the built-in `name`,
/// bypassing functions. A special built-in stays special: its errors exit
/// the shell.
pub fn builtin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let args = match argv.get(1) {
        Some(a) if a == b"--" => &argv[2..],
        _ => &argv[1..],
    };
    let Some(name) = args.first() else { return Ok(0) };
    match sh.builtin(name) {
        Some((f, _)) => sh.call_builtin(f, args),
        None if sh.plugins.as_ref().is_some_and(|h| h.has_builtin(name)) => crate::plugins::run_builtin(sh, args),
        None => {
            sh.berr(
                &argv[0],
                format!("{}: not a shell builtin", String::from_utf8_lossy(name)),
            );
            Ok(1)
        }
    }
}

/// `let`, as in zsh: evaluates each argument as `$((...))` does. The status
/// is 0 if the last value is non-zero, 1 if it is zero or on an error, which
/// stops at that argument.
pub fn let_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let args = match argv.get(1) {
        Some(a) if a == b"--" => &argv[2..],
        _ => &argv[1..],
    };
    if args.is_empty() {
        sh.berr(&argv[0], "not enough arguments");
        return Ok(1);
    }
    let mut v = 0;
    for a in args {
        match crate::expand::arith::eval(sh, a) {
            Ok(n) => v = n,
            Err(msg) => {
                sh.berr(&argv[0], msg);
                return Ok(1);
            }
        }
    }
    Ok((v == 0) as i32)
}

pub fn type_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut status = 0;
    for name in &argv[1..] {
        status |= describe(sh, name, true, None);
    }
    Ok(status)
}

pub fn hash(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() == 1 {
        let mut entries: Vec<_> = sh.hash.values().map(|(p, _)| p.clone()).collect();
        entries.sort();
        let mut out = Vec::new();
        for p in entries {
            out.extend(p);
            out.push(b'\n');
        }
        return Ok(sh.out_status(&out));
    }
    let mut status = 0;
    for a in &argv[1..] {
        if a == b"-r" {
            sh.hash.clear();
            continue;
        }
        let ext = sh.plugins.as_ref().is_some_and(|h| h.has_builtin(a));
        if a.contains(&b'/') || sh.builtin(a).is_some() || sh.functions.contains_key(a) || ext {
            continue;
        }
        sh.hash.remove(a);
        if sh.find_in_path(a).is_none() {
            sh.berr(&argv[0], format!("{}: not found", String::from_utf8_lossy(a)));
            status = 1;
        }
    }
    Ok(status)
}

/// The type of `getrlimit`'s resource argument, which differs between glibc and musl.
#[cfg(target_env = "gnu")]
type Resource = libc::__rlimit_resource_t;
#[cfg(not(target_env = "gnu"))]
type Resource = libc::c_int;

const LIMITS: &[(u8, Resource, u64, &str)] = &[
    (b't', libc::RLIMIT_CPU, 1, "time(seconds)"),
    (b'f', libc::RLIMIT_FSIZE, 512, "file(blocks)"),
    (b'd', libc::RLIMIT_DATA, 1024, "data(kbytes)"),
    (b's', libc::RLIMIT_STACK, 1024, "stack(kbytes)"),
    (b'c', libc::RLIMIT_CORE, 512, "coredump(blocks)"),
    (b'm', libc::RLIMIT_RSS, 1024, "memory(kbytes)"),
    (b'l', libc::RLIMIT_MEMLOCK, 1024, "locked memory(kbytes)"),
    (b'p', libc::RLIMIT_NPROC, 1, "process"),
    (b'n', libc::RLIMIT_NOFILE, 1, "nofiles"),
    (b'v', libc::RLIMIT_AS, 1024, "vmemory(kbytes)"),
    (b'w', libc::RLIMIT_LOCKS, 1, "locks"),
    (b'r', libc::RLIMIT_RTPRIO, 1, "rtprio"),
];

/// A port of dash's `ulimitcmd`.
pub fn ulimit(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    // As in dash: -H and -S select one limit (the last wins); by default a
    // value sets both, and the soft limit is shown.
    let (mut hard, mut soft) = (true, true);
    let mut all = false;
    let mut which = b'f';
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a == b"--" {
            i += 1;
            break;
        }
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        for &c in &a[1..] {
            match c {
                b'H' => (hard, soft) = (true, false),
                b'S' => (hard, soft) = (false, true),
                b'a' => all = true,
                c if LIMITS.iter().any(|l| l.0 == c) => which = c,
                c => {
                    sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                    return Ok(2);
                }
            }
        }
        i += 1;
    }
    let &(_, res, unit, _) = LIMITS.iter().find(|l| l.0 == which).unwrap();
    let value = match argv.get(i) {
        None => None,
        Some(_) if all || argv.len() > i + 1 => {
            sh.berr(&argv[0], "too many arguments");
            return Ok(2);
        }
        Some(v) if v == b"unlimited" => Some(libc::RLIM_INFINITY),
        Some(v) => {
            let mut n: u64 = 0;
            for &c in v.iter() {
                if !c.is_ascii_digit() {
                    sh.berr(&argv[0], "bad number");
                    return Ok(2);
                }
                n = n.wrapping_mul(10).wrapping_add((c - b'0') as u64);
            }
            Some(n.wrapping_mul(unit))
        }
    };
    let get = |res| {
        // SAFETY: valid output buffer.
        unsafe {
            let mut rl: libc::rlimit = std::mem::zeroed();
            libc::getrlimit(res, &mut rl);
            rl
        }
    };
    let show = |rl: libc::rlimit, unit: u64| {
        let v = if soft { rl.rlim_cur } else { rl.rlim_max };
        if v == libc::RLIM_INFINITY {
            "unlimited\n".to_string()
        } else {
            format!("{}\n", v / unit)
        }
    };
    if all {
        let mut out = String::new();
        for &(_, res, unit, desc) in LIMITS {
            out.push_str(&format!("{desc:<20} {}", show(get(res), unit)));
        }
        return Ok(sh.out_status(out.as_bytes()));
    }
    let Some(n) = value else {
        return Ok(sh.out_status(show(get(res), unit).as_bytes()));
    };
    let mut rl = get(res);
    if hard {
        rl.rlim_max = n;
    }
    if soft {
        rl.rlim_cur = n;
    }
    // SAFETY: valid rlimit.
    if unsafe { libc::setrlimit(res, &rl) } < 0 {
        sh.berr(
            &argv[0],
            format!("error setting limit ({})", sys::strerror(sys::errno())),
        );
        return Ok(2);
    }
    Ok(0)
}

/// A port of dash's `umaskcmd`.
pub fn umask(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut symbolic = false;
    let mut args = &argv[1..];
    while let Some(a) = args.first() {
        if a == b"--" {
            args = &args[1..];
            break;
        }
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        for &c in &a[1..] {
            if c != b'S' {
                sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                return Ok(2);
            }
            symbolic = true;
        }
        args = &args[1..];
    }
    let cur = sys::umask(0);
    sys::umask(cur);
    let Some(spec) = args.first() else {
        let out = if symbolic {
            let perm = !cur & 0o777;
            let part = |shift: u32| {
                let p = (perm >> shift) & 7;
                let mut s = String::new();
                if p & 4 != 0 {
                    s.push('r');
                }
                if p & 2 != 0 {
                    s.push('w');
                }
                if p & 1 != 0 {
                    s.push('x');
                }
                s
            };
            format!("u={},g={},o={}\n", part(6), part(3), part(0))
        } else {
            format!("{cur:04o}\n")
        };
        return Ok(sh.out_status(out.as_bytes()));
    };
    let new_mask = if spec.first().is_some_and(|c| c.is_ascii_digit()) {
        let mut m: u32 = 0;
        for &c in spec.iter() {
            if !(b'0'..b'8').contains(&c) {
                sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(spec)));
                return Ok(2);
            }
            m = (m << 3).wrapping_add((c - b'0') as u32);
        }
        m
    } else {
        let mask = !cur;
        let mut new = mask;
        let mut positions = 0;
        let mut i = 0;
        let at = |i: usize| spec.get(i).copied().unwrap_or(0);
        while at(i) != 0 {
            while at(i) != 0 && b"augo".contains(&at(i)) {
                positions |= match at(i) {
                    b'a' => 0o111,
                    b'u' => 0o100,
                    b'g' => 0o010,
                    _ => 0o001,
                };
                i += 1;
            }
            if positions == 0 {
                positions = 0o111;
            }
            let op = at(i);
            if op == 0 || !b"=+-".contains(&op) {
                break;
            }
            i += 1;
            let mut val = 0;
            while at(i) != 0 && b"rwxugoXs".contains(&at(i)) {
                val |= match at(i) {
                    b'r' => 4,
                    b'w' => 2,
                    b'x' => 1,
                    b'u' => mask >> 6,
                    b'g' => mask >> 3,
                    b'o' => mask,
                    b'X' if mask & 0o111 != 0 => 1,
                    _ => 0,
                };
                i += 1;
            }
            let val = (val & 7) * positions;
            match op {
                b'-' => new &= !val,
                b'=' => new = val | (new & !(positions * 7)),
                _ => new |= val,
            }
            if at(i) == b',' {
                positions = 0;
                i += 1;
            } else if at(i) == 0 || !b"=+-".contains(&at(i)) {
                break;
            }
        }
        if at(i) != 0 {
            sh.berr(&argv[0], format!("Illegal mode: {}", String::from_utf8_lossy(spec)));
            return Ok(2);
        }
        !new
    };
    sys::umask(new_mask & 0o7777);
    Ok(0)
}
