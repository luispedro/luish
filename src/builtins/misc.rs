//! `.`, `times`, `alias`, `unalias`, `command`, `type`, `hash`, `ulimit`,
//! `umask`.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;

use super::vars::single_quote;
use crate::exec::CommandKind;
use crate::parser::is_reserved;
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

pub fn dot(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let Some(name) = argv.get(1) else {
        sh.berr(&argv[0], "filename argument required");
        return Err(Flow::Error(2));
    };
    let path = if name.contains(&b'/') {
        Some(name.clone())
    } else {
        let path = sh.get_var(b"PATH").unwrap_or_default();
        path.split(|&c| c == b':').find_map(|dir| {
            let mut p = if dir.is_empty() { b".".to_vec() } else { dir.to_vec() };
            p.push(b'/');
            p.extend_from_slice(name);
            sys::stat(&p)
                .is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG)
                .then_some(p)
        })
    };
    if let (Some(rec), Some(p)) = (&mut sh.sourced_files, &path) {
        rec.push(match &sh.curdir {
            Some(dir) if p.first() != Some(&b'/') => [dir.as_slice(), b"/", p].concat(),
            _ => p.clone(),
        });
    }
    let text = match path.map(|p| std::fs::read(OsStr::from_bytes(&p))) {
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
    let r = sh.run_string(&text);
    sh.dot_depth -= 1;
    sh.lineno = saved_lineno;
    match r {
        Err(Flow::Return(n)) => Ok(n),
        r => r,
    }
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

pub fn alias(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() == 1 {
        let mut names: Vec<_> = sh.aliases.iter().collect();
        names.sort();
        let out: Vec<u8> = names.into_iter().flat_map(|(n, v)| alias_line(n, v)).collect();
        return Ok(sh.out_status(&out));
    }
    let mut status = 0;
    for a in &argv[1..] {
        match a.iter().position(|&c| c == b'=') {
            Some(i) if i > 0 => {
                Rc::make_mut(&mut sh.aliases).insert(a[..i].to_vec(), a[i + 1..].to_vec());
            }
            _ => match sh.aliases.get(a) {
                Some(v) => {
                    sh.out(&alias_line(a, v));
                }
                None => {
                    sh.berr(&argv[0], format!("{}: not found", String::from_utf8_lossy(a)));
                    status = 1;
                }
            },
        }
    }
    Ok(status)
}

pub fn unalias(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.get(1).is_some_and(|a| a == b"-a") {
        Rc::make_mut(&mut sh.aliases).clear();
        return Ok(0);
    }
    let mut status = 0;
    for a in &argv[1..] {
        if Rc::make_mut(&mut sh.aliases).remove(a).is_none() {
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
    } else if let Some(v) = sh.aliases.get(name) {
        if verbose {
            format!("{n} is an alias for {}", String::from_utf8_lossy(v)).into_bytes()
        } else {
            let mut s = b"alias ".to_vec();
            let mut l = alias_line(name, v);
            l.pop();
            s.extend(l);
            s
        }
    } else {
        match sh.lookup_command(name, true) {
            CommandKind::Special(_) => line("is a special shell builtin"),
            CommandKind::Function(_) => line("is a shell function"),
            CommandKind::Builtin(_) => line("is a shell builtin"),
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
        if a.contains(&b'/') || super::lookup(a).is_some() || sh.functions.contains_key(a) {
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

const LIMITS: &[(u8, libc::__rlimit_resource_t, u64, &str)] = &[
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
