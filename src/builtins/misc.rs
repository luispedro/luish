//! `.`, `times`, `alias`, `unalias`, `command`, `type`, `hash`, `jobs`,
//! `ulimit`, `umask`.

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
    let text = path.and_then(|p| std::fs::read(OsStr::from_bytes(&p)).ok());
    let Some(text) = text else {
        sh.berr(
            &argv[0],
            format!("cannot open {}: No such file", String::from_utf8_lossy(name)),
        );
        return Err(Flow::Error(2));
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

pub fn times(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
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
    Ok(sh.out_or_err(&argv[0], out.as_bytes()))
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
        return Ok(sh.out_or_err(&argv[0], &out));
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
fn describe(sh: &mut Shell, name: &[u8], verbose: bool, default_path: bool) -> Option<Vec<u8>> {
    let n = String::from_utf8_lossy(name);
    if is_reserved(name) {
        return Some(
            if verbose {
                format!("{n} is a shell keyword")
            } else {
                n.to_string()
            }
            .into_bytes(),
        );
    }
    if let Some(v) = sh.aliases.get(name) {
        return Some(if verbose {
            format!("{n} is an alias for {}", String::from_utf8_lossy(v)).into_bytes()
        } else {
            let mut s = b"alias ".to_vec();
            let mut l = alias_line(name, v);
            l.pop();
            s.extend(l);
            s
        });
    }
    match sh.lookup_command(name, true) {
        CommandKind::Special(_) => {
            return Some(
                if verbose {
                    format!("{n} is a special shell builtin")
                } else {
                    n.to_string()
                }
                .into_bytes(),
            );
        }
        CommandKind::Function(_) => {
            return Some(
                if verbose {
                    format!("{n} is a shell function")
                } else {
                    n.to_string()
                }
                .into_bytes(),
            );
        }
        CommandKind::Builtin(_) => {
            return Some(
                if verbose {
                    format!("{n} is a shell builtin")
                } else {
                    n.to_string()
                }
                .into_bytes(),
            );
        }
        CommandKind::External => {}
    }
    let path = if name.contains(&b'/') {
        sys::access(name, libc::X_OK).then(|| name.to_vec())
    } else if default_path {
        let saved = sh.get_var(b"PATH");
        let _ = sh.vars.set(b"PATH", b"/usr/bin:/bin".to_vec());
        let p = sh.search_path(name);
        match saved {
            Some(s) => {
                let _ = sh.vars.set(b"PATH", s);
            }
            None => {
                let _ = sh.vars.unset(b"PATH");
            }
        }
        p
    } else {
        sh.find_in_path(name)
    }?;
    Some(if verbose {
        let mut s = format!("{n} is ").into_bytes();
        s.extend(path);
        s
    } else {
        path
    })
}

pub fn command(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut i = 1;
    let mut verbose = None;
    let mut default_path = false;
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
                b'v' => verbose = Some(false),
                b'V' => verbose = Some(true),
                b'p' => default_path = true,
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
        let mut status = 0;
        for name in args {
            match describe(sh, name, verbose, default_path) {
                Some(mut d) => {
                    d.push(b'\n');
                    sh.out(&d);
                }
                None => {
                    if verbose {
                        sh.error(format!("{}: not found", String::from_utf8_lossy(name)));
                    }
                    status = 127;
                }
            }
        }
        return Ok(status);
    }
    // `command` suppresses function lookup, and errors from special
    // built-ins no longer exit the shell.
    match sh.run_argv(args, false) {
        Err(Flow::Error(n)) => Ok(n),
        r => r,
    }
}

pub fn type_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut status = 0;
    for name in &argv[1..] {
        match describe(sh, name, true, false) {
            Some(mut d) => {
                d.push(b'\n');
                sh.out(&d);
            }
            None => {
                sh.out(format!("{}: not found\n", String::from_utf8_lossy(name)).as_bytes());
                status = 127;
            }
        }
    }
    Ok(status)
}

pub fn hash(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() == 1 {
        let mut entries: Vec<_> = sh.hash.values().cloned().collect();
        entries.sort();
        let mut out = Vec::new();
        for p in entries {
            out.extend(p);
            out.push(b'\n');
        }
        return Ok(sh.out_or_err(&argv[0], &out));
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

pub fn jobs(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    sh.jobs.reap();
    let mut out = String::new();
    let n = sh.jobs.jobs.len();
    for (i, j) in sh.jobs.jobs.iter().enumerate() {
        let mark = if i + 1 == n {
            '+'
        } else if i + 2 == n {
            '-'
        } else {
            ' '
        };
        let state = match j.status() {
            Some(0) if j.done() => "Done".to_string(),
            Some(s) if j.done() => format!("Done({s})"),
            _ => "Running".to_string(),
        };
        out.push_str(&format!("[{}] {} {:<24}{}\n", j.id, mark, state, j.cmd));
    }
    sh.jobs.jobs.retain(|j| !j.done());
    Ok(sh.out_or_err(&argv[0], out.as_bytes()))
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

pub fn ulimit(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut hard = false;
    let mut soft = false;
    let mut all = false;
    let mut which = b'f';
    let mut value = None;
    for a in &argv[1..] {
        if a.len() > 1 && a[0] == b'-' {
            for &c in &a[1..] {
                match c {
                    b'H' => hard = true,
                    b'S' => soft = true,
                    b'a' => all = true,
                    c if LIMITS.iter().any(|l| l.0 == c) => which = c,
                    c => {
                        sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                        return Ok(2);
                    }
                }
            }
        } else {
            value = Some(a.clone());
        }
    }
    if !hard && !soft {
        soft = true;
    }
    let get = |res| {
        // SAFETY: valid output buffer.
        unsafe {
            let mut rl: libc::rlimit = std::mem::zeroed();
            libc::getrlimit(res, &mut rl);
            rl
        }
    };
    let show = |rl: libc::rlimit, unit: u64| {
        let v = if hard && !soft { rl.rlim_max } else { rl.rlim_cur };
        if v == libc::RLIM_INFINITY {
            "unlimited".to_string()
        } else {
            (v / unit).to_string()
        }
    };
    if all {
        let mut out = String::new();
        for &(c, res, unit, desc) in LIMITS {
            out.push_str(&format!("-{} {:<24}{}\n", c as char, desc, show(get(res), unit)));
        }
        return Ok(sh.out_or_err(&argv[0], out.as_bytes()));
    }
    let &(_, res, unit, _) = LIMITS.iter().find(|l| l.0 == which).unwrap();
    match value {
        None => {
            let out = format!("{}\n", show(get(res), unit));
            Ok(sh.out_or_err(&argv[0], out.as_bytes()))
        }
        Some(v) => {
            let n = if v == b"unlimited" {
                libc::RLIM_INFINITY
            } else {
                match super::parse_uint(&v) {
                    Some(n) => (n as u64).saturating_mul(unit),
                    None => {
                        sh.berr(&argv[0], format!("bad number: {}", String::from_utf8_lossy(&v)));
                        return Ok(2);
                    }
                }
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
    }
}

/// Applies a symbolic mode like `u=rwx,g+w,o-r` to permission bits.
fn symbolic_mode(spec: &[u8], mut perm: u32) -> Option<u32> {
    for clause in spec.split(|&c| c == b',') {
        let mut i = 0;
        let mut who = 0u32;
        while i < clause.len() && b"ugoa".contains(&clause[i]) {
            who |= match clause[i] {
                b'u' => 0o700,
                b'g' => 0o070,
                b'o' => 0o007,
                _ => 0o777,
            };
            i += 1;
        }
        if who == 0 {
            who = 0o777;
        }
        if i >= clause.len() {
            return None;
        }
        while i < clause.len() {
            let op = clause[i];
            if !b"+-=".contains(&op) {
                return None;
            }
            i += 1;
            let mut bits = 0u32;
            while i < clause.len() && b"rwx".contains(&clause[i]) {
                bits |= match clause[i] {
                    b'r' => 0o444,
                    b'w' => 0o222,
                    _ => 0o111,
                };
                i += 1;
            }
            let bits = bits & who;
            match op {
                b'+' => perm |= bits,
                b'-' => perm &= !bits,
                _ => perm = (perm & !who) | bits,
            }
        }
    }
    Some(perm)
}

pub fn umask(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut symbolic = false;
    let mut args = &argv[1..];
    if args.first().is_some_and(|a| a == b"-S") {
        symbolic = true;
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
        return Ok(sh.out_or_err(&argv[0], out.as_bytes()));
    };
    let new = if spec.iter().all(|c| (b'0'..=b'7').contains(c)) {
        u32::from_str_radix(std::str::from_utf8(spec).unwrap(), 8).ok()
    } else {
        symbolic_mode(spec, !cur & 0o777).map(|p| !p & 0o777)
    };
    match new {
        Some(m) => {
            sys::umask(m);
            Ok(0)
        }
        None => {
            sh.berr(&argv[0], format!("Illegal mode: {}", String::from_utf8_lossy(spec)));
            Ok(2)
        }
    }
}
