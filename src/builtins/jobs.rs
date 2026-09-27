//! `jobs`, `fg`, `bg`, `wait`, and `kill`.

use super::options;
use crate::jobs::{JobState, ShowMode, Waited};
use crate::shell::{ExecResult, Shell};
use crate::signals;
use crate::sys;

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
    Ok(sh.out_status(out.as_bytes()))
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

/// A port of dash's `killcmd` (except for jobs started without job
/// control: see docs/compatibility.md).
pub fn kill(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let usage = |sh: &Shell| {
        sh.berr(
            &argv[0],
            "Usage: kill [-s sigspec | -signum | -sigspec] [pid | job]... or\nkill -l [exitstatus]",
        );
        Ok(2)
    };
    let mut sig = None;
    let mut list = false;
    let mut i = 1;
    let Some(first) = argv.get(1) else {
        return usage(sh);
    };
    if first.first() == Some(&b'-') {
        sig = signals::parse(&first[1..], 1);
        if sig.is_some() {
            i = 2;
        } else {
            // dash's `nextopt("ls:")`.
            'opts: while let Some(a) = argv.get(i) {
                if a.len() < 2 || a[0] != b'-' {
                    break;
                }
                i += 1;
                if a == b"--" {
                    break;
                }
                let mut j = 1;
                while j < a.len() {
                    match a[j] {
                        b'l' => list = true,
                        b's' => {
                            let name = if j + 1 < a.len() {
                                a[j + 1..].to_vec()
                            } else if let Some(n) = argv.get(i) {
                                i += 1;
                                n.clone()
                            } else {
                                sh.berr(&argv[0], "No arg for -s option");
                                return Ok(2);
                            };
                            sig = signals::parse(&name, 1);
                            if sig.is_none() {
                                sh.berr(
                                    &argv[0],
                                    format!("invalid signal number or name: {}", String::from_utf8_lossy(&name)),
                                );
                                return Ok(2);
                            }
                            continue 'opts;
                        }
                        c => {
                            sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                            return Ok(2);
                        }
                    }
                    j += 1;
                }
            }
        }
    }
    let args = &argv[i..];
    if !list && sig.is_none() {
        sig = Some(libc::SIGTERM);
    }
    if (sig.is_none() || args.is_empty()) ^ list {
        return usage(sh);
    }
    if list {
        let mut out = String::new();
        match args.first() {
            None => {
                out.push_str("0\n");
                for s in 1..signals::NSIG as i32 {
                    out.push_str(&signals::name(s));
                    out.push('\n');
                }
            }
            Some(a) => {
                let Some(n) = super::parse_uint(a) else {
                    sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(a)));
                    return Ok(2);
                };
                let n = if n > 128 { n - 128 } else { n };
                if n <= 0 || n >= signals::NSIG as i64 {
                    sh.berr(
                        &argv[0],
                        format!("invalid signal number or exit status: {}", String::from_utf8_lossy(a)),
                    );
                    return Ok(2);
                }
                out.push_str(&format!("{}\n", signals::name(n as i32)));
            }
        }
        return Ok(sh.out_status(out.as_bytes()));
    }
    let sig = sig.unwrap();
    let mut status = 0;
    for a in args {
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
            let (neg, digits) = match a.strip_prefix(b"-") {
                Some(d) => (true, d),
                None => (false, &a[..]),
            };
            match super::parse_uint(digits) {
                Some(p) if p <= i32::MAX as i64 => vec![if neg { -(p as i32) } else { p as i32 }],
                _ => {
                    sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(a)));
                    return Ok(2);
                }
            }
        };
        for pid in pids {
            if let Err(e) = sys::kill(pid, sig) {
                sh.berr(&argv[0], sys::strerror(e));
                status = 1;
            }
        }
    }
    Ok(status)
}
