//! `jobs`, `fg`, `bg`, `wait`, and `kill`.

use crate::jobs::{JobState, ShowMode, Waited};
use crate::shell::{ExecResult, Shell};
use crate::signals;
use crate::sys;

/// Parses the options of `jobs`, `fg`, `bg` and `wait` (dash's `nextopt`).
/// Returns the option letters and the operands.
fn options<'a>(sh: &Shell, argv: &'a [Vec<u8>], allowed: &[u8]) -> Result<(Vec<u8>, &'a [Vec<u8>]), i32> {
    let mut opts = Vec::new();
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        for &c in &a[1..] {
            if !allowed.contains(&c) {
                sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                return Err(2);
            }
            opts.push(c);
        }
    }
    Ok((opts, &argv[i..]))
}

pub fn jobs(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match options(sh, argv, b"lp") {
        Ok(r) => r,
        Err(s) => return Ok(s),
    };
    let mode = match opts.last() {
        Some(b'l') => ShowMode::Pids,
        Some(b'p') => ShowMode::Pgid,
        _ => ShowMode::Normal,
    };
    let mut out = String::new();
    if args.is_empty() {
        sh.reap_jobs();
        for i in sh.jobs.order().to_vec() {
            out.push_str(&sh.show_job(i, mode));
        }
    } else {
        for a in args {
            match sh.get_job(Some(a), false) {
                Ok(i) => out.push_str(&sh.show_job(i, mode)),
                Err(msg) => {
                    sh.out(out.as_bytes());
                    sh.berr(&argv[0], msg);
                    return Ok(2);
                }
            }
        }
    }
    Ok(sh.out_or_err(&argv[0], out.as_bytes()))
}

/// `fg` and `bg`.
pub fn fg(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let foreground = argv[0] == b"fg";
    let args = match options(sh, argv, b"") {
        Ok((_, a)) => a,
        Err(s) => return Ok(s),
    };
    let specs: Vec<Option<&[u8]>> = if args.is_empty() {
        vec![None]
    } else {
        args.iter().map(|a| Some(&a[..])).collect()
    };
    let mut status = 0;
    for spec in specs {
        let i = match sh.get_job(spec, true) {
            Ok(i) => i,
            Err(msg) => {
                sh.berr(&argv[0], msg);
                return Ok(2);
            }
        };
        let mut line = String::new();
        if !foreground {
            sh.jobs.make_current(i, false);
            line.push_str(&format!("[{}] ", crate::jobs::JobTable::number(i)));
        }
        line.push_str(&sh.jobs.get(i).text());
        line.push('\n');
        sh.out(line.as_bytes());
        status = sh.restart_job(i, foreground);
    }
    Ok(status)
}

pub fn wait(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let args = match options(sh, argv, b"") {
        Ok((_, a)) => a,
        Err(s) => return Ok(s),
    };
    if args.is_empty() {
        loop {
            // Wait until no job is running. The finished ones are now
            // "waited for", so they can be freed when a new job is made.
            let mut running = false;
            for i in sh.jobs.order().to_vec() {
                let job = sh.jobs.get_mut(i);
                if job.state == JobState::Running {
                    running = true;
                    break;
                }
                job.waited = true;
            }
            if !running {
                return Ok(0);
            }
            match sh.wait_any(true)? {
                Waited::Child(..) => {}
                Waited::NoChildren => return Ok(0),
                Waited::Interrupted(sig) => return Ok(128 + sig),
            }
        }
    }
    let mut status = 127;
    for a in args {
        let i = if a.first() == Some(&b'%') {
            match sh.get_job(Some(a), false) {
                Ok(i) => i,
                Err(msg) => {
                    sh.berr(&argv[0], msg);
                    return Ok(2);
                }
            }
        } else {
            let Some(pid) = super::parse_uint(a) else {
                sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(a)));
                return Ok(2);
            };
            // As in dash, only the last process of a job can be named.
            match sh.jobs.find_last_pid(pid as i32) {
                Some(i) => i,
                None => {
                    status = 127;
                    continue;
                }
            }
        };
        while sh.jobs.get(i).state == JobState::Running {
            match sh.wait_any(true)? {
                Waited::Child(pid, ws) => {
                    if sh.jobs.get(i).procs.iter().any(|p| p.pid == pid) {
                        crate::exec::report_signaled(&ws);
                    }
                }
                Waited::NoChildren => break,
                Waited::Interrupted(sig) => return Ok(128 + sig),
            }
        }
        let job = sh.jobs.get_mut(i);
        job.waited = true;
        status = match job.procs.last().and_then(|p| p.status) {
            Some(ws) => ws.code(),
            None => 127,
        };
    }
    Ok(status)
}

pub fn kill(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let usage = |sh: &Shell| {
        sh.berr(
            &argv[0],
            "usage: kill [-s sigspec | -signum | -sigspec] [pid | job]... or\nkill -l [exitstatus]",
        );
        Ok(2)
    };
    let mut sig = libc::SIGTERM;
    let mut i = 1;
    let Some(first) = argv.get(1) else {
        return usage(sh);
    };
    if first == b"-l" || first == b"-L" {
        let mut out = String::new();
        match argv.get(2) {
            Some(a) => {
                let n = super::parse_uint(a).unwrap_or(-1) as i32;
                let n = if n > 128 { n - 128 } else { n };
                match (n > 0).then(|| signals::name(n)) {
                    Some(name) if !name.chars().all(|c| c.is_ascii_digit()) => out.push_str(&format!("{name}\n")),
                    _ => {
                        sh.berr(
                            &argv[0],
                            format!("invalid signal number or exit status: {}", String::from_utf8_lossy(a)),
                        );
                        return Ok(1);
                    }
                }
            }
            None => {
                for s in signals::all() {
                    out.push_str(&signals::name(s));
                    out.push('\n');
                }
            }
        }
        return Ok(sh.out_or_err(&argv[0], out.as_bytes()));
    }
    if first == b"-s" || first == b"-n" {
        let Some(name) = argv.get(2) else {
            return usage(sh);
        };
        match signals::parse(name) {
            Some(s) => sig = s,
            None => {
                sh.berr(
                    &argv[0],
                    format!("invalid signal number or name: {}", String::from_utf8_lossy(name)),
                );
                return Ok(1);
            }
        }
        i = 3;
    } else if first.len() > 1 && first[0] == b'-' && first != b"--" {
        match signals::parse(&first[1..]) {
            Some(s) => sig = s,
            None => {
                sh.berr(
                    &argv[0],
                    format!(
                        "invalid signal number or name: {}",
                        String::from_utf8_lossy(&first[1..])
                    ),
                );
                return Ok(1);
            }
        }
        i = 2;
    }
    if argv.get(i).is_some_and(|a| a == b"--") {
        i += 1;
    }
    if i >= argv.len() {
        return usage(sh);
    }
    let mut status = 0;
    for a in &argv[i..] {
        let pids = if a.first() == Some(&b'%') {
            match sh.get_job(Some(a), false) {
                // A job under job control is its process group. Unlike dash,
                // signal the processes of any other job one by one (dash
                // signals a process group that doesn't exist).
                Ok(i) if sh.jobs.get(i).jobctl => vec![-sh.jobs.get(i).pgid()],
                Ok(i) => sh.jobs.get(i).procs.iter().map(|p| p.pid).collect(),
                Err(msg) => {
                    sh.berr(&argv[0], msg);
                    return Ok(2);
                }
            }
        } else {
            match std::str::from_utf8(a).ok().and_then(|s| s.parse::<i32>().ok()) {
                Some(p) => vec![p],
                None => {
                    sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(a)));
                    status = 1;
                    continue;
                }
            }
        };
        for pid in pids {
            if let Err(e) = sys::kill(pid, sig) {
                sh.berr(&argv[0], format!("{pid}: {}", sys::strerror(e)));
                status = 1;
            }
        }
    }
    Ok(status)
}
