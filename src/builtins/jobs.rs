//! `jobs`, `fg`, `bg`, `wait`, `disown`, and `kill`.

use super::options;
use crate::jobs::{JobState, ShowMode, Waited};
use crate::lexer::is_valid_name;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals;
use crate::sys;

pub fn jobs(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match options(sh, argv, b"ilp") {
        Ok(r) => r,
        Err(s) => return Ok(s),
    };
    if opts.contains(&b'i') {
        return interactive(sh, argv, args);
    }
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

/// `jobs -i [job]`: the menu of the jobs (`interactive/jobmenu.rs`), with
/// `job` selected.
fn interactive(sh: &mut Shell, argv: &[Vec<u8>], args: &[Vec<u8>]) -> ExecResult {
    use crate::interactive::jobmenu;
    if args.len() > 1 {
        sh.berr(&argv[0], "-i takes at most one job");
        return Ok(2);
    }
    let first = match args.first().map(|a| sh.get_job(Some(a), false)) {
        Some(Err(msg)) => {
            sh.berr(&argv[0], msg);
            return Ok(2);
        }
        Some(Ok(i)) => Some(i),
        None => None,
    };
    if !jobmenu::usable(sh) {
        sh.berr(&argv[0], "-i needs a terminal");
        return Ok(2);
    }
    Ok(jobmenu::run(sh, first))
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

/// `wait`, with bash's options (not POSIX): `-n` waits for the first job to
/// finish, `-p VAR` sets `VAR` to the pid of the job whose status is
/// returned, and `-f` waits for stopped jobs to finish too.
pub fn wait(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (mut any, mut force, mut var) = (false, false, None);
    let mut n = 1;
    while let Some(a) = argv.get(n) {
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        n += 1;
        if a == b"--" {
            break;
        }
        for (j, &c) in a.iter().enumerate().skip(1) {
            match c {
                b'n' => any = true,
                b'f' => force = true,
                b'p' => {
                    var = if j + 1 < a.len() {
                        Some(&a[j + 1..])
                    } else if let Some(v) = argv.get(n) {
                        n += 1;
                        Some(&v[..])
                    } else {
                        sh.berr(&argv[0], "No arg for -p option");
                        return Ok(2);
                    };
                    break;
                }
                _ => {
                    sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                    return Ok(2);
                }
            }
        }
    }
    let args = &argv[n..];
    if let Some(v) = var {
        if !is_valid_name(v) {
            sh.berr(&argv[0], format!("{}: bad variable name", String::from_utf8_lossy(v)));
            return Ok(2);
        }
        if sh.unset_var(v).is_err() {
            sh.berr(&argv[0], format!("{}: is read only", String::from_utf8_lossy(v)));
            return Err(Flow::Error(2));
        }
    }
    // A stopped job is still waited for with `-f`, and never with `-n`.
    let waiting = |state| state == JobState::Running || force && state == JobState::Stopped;
    if any {
        let mut jobs = Vec::new();
        for a in args {
            match wait_operand(sh, &argv[0], a) {
                Ok(Some(i)) => jobs.push(i),
                Ok(None) => {}
                Err(s) => return Ok(s),
            }
        }
        loop {
            let candidates = if args.is_empty() { sh.jobs.order() } else { &jobs[..] };
            // Of the jobs that have finished, the one with the lowest
            // number, as in bash.
            let done = (candidates.iter().copied())
                .filter(|&i| sh.jobs.get(i).state == JobState::Done && !sh.jobs.get(i).waited)
                .min();
            if let Some(i) = done {
                return wait_status(sh, i, var);
            }
            if !candidates.iter().any(|&i| sh.jobs.get(i).state == JobState::Running) {
                return Ok(127);
            }
            match sh.wait_any(true)? {
                Waited::Child(pid, ws) => {
                    if sh.jobs.find_pid(pid).is_some() {
                        crate::exec::report_signaled(&ws);
                    }
                }
                Waited::NoChildren => return Ok(127),
                Waited::Interrupted(sig) => return Ok(128 + sig),
            }
        }
    }
    if args.is_empty() {
        loop {
            // Wait until no job is running. The finished ones are now
            // "waited for", so they can be freed when a new job is made.
            let mut running = false;
            for i in sh.jobs.order().to_vec() {
                let job = sh.jobs.get_mut(i);
                if waiting(job.state) {
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
    for (k, a) in args.iter().enumerate() {
        let i = match wait_operand(sh, &argv[0], a) {
            Ok(Some(i)) => i,
            Ok(None) => {
                status = 127;
                continue;
            }
            Err(s) => return Ok(s),
        };
        while waiting(sh.jobs.get(i).state) {
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
        // `-p` names the job of the last operand only.
        status = wait_status(sh, i, var.filter(|_| k + 1 == args.len()))?;
    }
    Ok(status)
}

/// The job that an operand of `wait` names: `Ok(None)` for a pid that isn't
/// a child, or the exit status after an error.
fn wait_operand(sh: &Shell, name: &[u8], a: &[u8]) -> Result<Option<usize>, i32> {
    if a.first() == Some(&b'%') {
        return match sh.get_job(Some(a), false) {
            Ok(i) => Ok(Some(i)),
            Err(msg) => {
                sh.berr(name, msg);
                Err(2)
            }
        };
    }
    let Some(pid) = super::parse_uint(a) else {
        sh.berr(name, format!("Illegal number: {}", String::from_utf8_lossy(a)));
        return Err(2);
    };
    // As in dash, only the last process of a job can be named.
    Ok(sh.jobs.find_last_pid(pid as i32))
}

/// Marks job `i` as waited for and returns its status, setting the variable
/// of `-p` (if any) to the pid of its last process.
fn wait_status(sh: &mut Shell, i: usize, var: Option<&[u8]>) -> ExecResult {
    let job = sh.jobs.get_mut(i);
    job.waited = true;
    let Some(last) = job.procs.last().filter(|p| p.status.is_some()) else {
        return Ok(127);
    };
    let pid = last.pid;
    let status = job.status();
    if let Some(v) = var {
        sh.set_var(v, pid.to_string().into_bytes())?;
    }
    Ok(status)
}

/// `disown` (not POSIX; as in zsh and bash): removes jobs from the table,
/// so that they are no longer reported, waited for, warned about or sent
/// SIGHUP. With bash's `-h`, they stay but aren't sent SIGHUP.
pub fn disown(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match options(sh, argv, b"ahr") {
        Ok(r) => r,
        Err(s) => return Ok(s),
    };
    let running = opts.contains(&b'r');
    let mut status = 0;
    let mut slots = Vec::new();
    if opts.contains(&b'a') || (running && args.is_empty()) {
        slots = sh.jobs.order().to_vec();
    } else if args.is_empty() {
        match sh.get_job(None, false) {
            Ok(i) => slots.push(i),
            Err(msg) => {
                sh.berr(&argv[0], msg);
                return Ok(1);
            }
        }
    }
    for a in args {
        // A job, or as in bash, the process id of one of its processes.
        let found = if a.first() == Some(&b'%') {
            sh.get_job(Some(a), false)
        } else {
            match super::parse_uint(a) {
                Some(pid) => sh
                    .jobs
                    .find_pid(pid as i32)
                    .ok_or_else(|| format!("No such job: {}", text(a))),
                None => Err(format!("Illegal number: {}", text(a))),
            }
        };
        match found {
            Ok(i) if !slots.contains(&i) => slots.push(i),
            Ok(_) => {}
            Err(msg) => {
                sh.berr(&argv[0], msg);
                status = 1;
            }
        }
    }
    if opts.contains(&b'h') {
        for i in slots {
            sh.jobs.get_mut(i).nohup = true;
        }
        return Ok(status);
    }
    for i in slots {
        let job = sh.jobs.get(i);
        if running && job.state != JobState::Running {
            continue;
        }
        if job.state == JobState::Stopped {
            // As in zsh (which also names each process).
            let pids = if job.jobctl {
                format!(" -{}", job.pgid())
            } else {
                job.procs.iter().map(|p| format!(" {}", p.pid)).collect()
            };
            sh.berr(
                &argv[0],
                format!("warning: job is stopped, use `kill -CONT{pids}' to resume"),
            );
        }
        sh.jobs.free(i);
    }
    Ok(status)
}

fn text(s: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(s)
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
