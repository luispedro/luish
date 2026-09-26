//! `trap`, `kill`, and `wait`.

use super::vars::single_quote;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals::{self, Disposition, NSIG};
use crate::sys;

impl Shell {
    /// Sets the disposition of `sig` to match its trap (or the shell's
    /// default handling when the trap is reset).
    pub fn apply_trap_disposition(&self, sig: usize) {
        if sig == 0 {
            return;
        }
        let d = match &self.traps[sig] {
            Some(a) if a.is_empty() => Disposition::Ignore,
            Some(_) => Disposition::Catch,
            None => self.default_disposition(sig as i32),
        };
        signals::set_disposition(sig as i32, d);
    }

    pub fn default_disposition(&self, sig: i32) -> Disposition {
        if self.interactive {
            if sig == libc::SIGINT {
                return Disposition::Catch;
            }
            if sig == libc::SIGQUIT || sig == libc::SIGTERM {
                return Disposition::Ignore;
            }
            if self.opt(crate::options::Opt::Monitor) && matches!(sig, libc::SIGTSTP | libc::SIGTTIN | libc::SIGTTOU) {
                return Disposition::Ignore;
            }
        }
        Disposition::Default
    }
}

pub fn trap(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut args = &argv[1..];
    if args.first().is_some_and(|a| a == b"--") {
        args = &args[1..];
    }
    if args.is_empty() {
        let mut out = Vec::new();
        for (sig, t) in sh.traps.iter().enumerate() {
            if let Some(action) = t {
                out.extend_from_slice(b"trap -- ");
                out.extend(single_quote(action));
                out.push(b' ');
                out.extend_from_slice(signals::name(sig as i32).as_bytes());
                out.push(b'\n');
            }
        }
        return Ok(sh.out_or_err(&argv[0], &out));
    }
    // `trap N...` with an unsigned integer first operand resets.
    let (action, sigs) = if args.len() == 1 || super::parse_uint(&args[0]).is_some() {
        (None, args)
    } else if args[0] == b"-" {
        (None, &args[1..])
    } else {
        (Some(args[0].clone()), &args[1..])
    };
    let mut status = 0;
    for s in sigs {
        let Some(sig) = signals::parse(s) else {
            sh.berr(&argv[0], format!("{}: bad trap", String::from_utf8_lossy(s)));
            status = 1;
            continue;
        };
        let sig = sig as usize;
        if sig >= NSIG {
            continue;
        }
        if sig != 0 && sh.ignored_on_entry[sig] && !sh.interactive {
            continue;
        }
        sh.traps[sig] = action.clone();
        sh.apply_trap_disposition(sig);
    }
    Ok(status)
}

/// Resolves a job spec (`%n`, `%%`, `%+`, `%-`, `%str`) to process ids.
pub fn job_pids(sh: &Shell, spec: &[u8]) -> Option<Vec<i32>> {
    let rest = &spec[1..];
    let jobs = &sh.jobs.jobs;
    let job = match rest {
        b"" | b"%" | b"+" => jobs.last(),
        b"-" => jobs.len().checked_sub(2).map(|i| &jobs[i]),
        _ => {
            if let Some(n) = super::parse_uint(rest) {
                jobs.iter().find(|j| j.id as i64 == n)
            } else if let Some(s) = rest.strip_prefix(b"?") {
                jobs.iter().find(|j| j.cmd.as_bytes().windows(s.len()).any(|w| w == s))
            } else {
                jobs.iter().find(|j| j.cmd.as_bytes().starts_with(rest))
            }
        }
    }?;
    Some(job.procs.iter().map(|p| p.0).collect())
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
            match job_pids(sh, a) {
                Some(p) => p,
                None => {
                    sh.berr(&argv[0], format!("{}: no such job", String::from_utf8_lossy(a)));
                    status = 1;
                    continue;
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

/// Waits for one process. Returns its status, or `Err(128 + sig)` if a
/// trapped signal interrupted the wait.
fn wait_one(sh: &mut Shell, pid: i32) -> Result<Result<i32, i32>, Flow> {
    loop {
        if let Some(i) = sh.jobs.find_pid(pid)
            && let Some(p) = sh.jobs.jobs[i].procs.iter().find(|p| p.0 == pid)
            && let Some(s) = p.1
        {
            return Ok(Ok(s));
        }
        match sys::waitpid(pid, 0) {
            Ok(Some((p, ws))) => {
                crate::exec::report_signaled(&ws);
                sh.jobs.record(p, &ws);
            }
            Ok(None) => {}
            Err(libc::EINTR) => {
                let trapped = signals::peek_pending()
                    .into_iter()
                    .rev()
                    .find(|&s| sh.traps[s].as_ref().is_some_and(|a| !a.is_empty()));
                sh.run_pending_traps()?;
                if let Some(sig) = trapped {
                    return Ok(Err(128 + sig as i32));
                }
            }
            Err(_) => return Ok(Ok(127)),
        }
    }
}

pub fn wait(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut status = 0;
    if argv.len() == 1 {
        let pids: Vec<i32> = sh.jobs.jobs.iter().flat_map(|j| j.procs.iter().map(|p| p.0)).collect();
        for pid in pids {
            if let Err(s) = wait_one(sh, pid)? {
                return Ok(s);
            }
        }
        sh.jobs.clear();
        return Ok(0);
    }
    for a in &argv[1..] {
        let pids = if a.first() == Some(&b'%') {
            match job_pids(sh, a) {
                Some(p) => p,
                None => {
                    status = 127;
                    continue;
                }
            }
        } else {
            match std::str::from_utf8(a).ok().and_then(|s| s.parse::<i32>().ok()) {
                Some(p) => vec![p],
                None => {
                    sh.berr(&argv[0], format!("Illegal number: {}", String::from_utf8_lossy(a)));
                    return Ok(2);
                }
            }
        };
        for pid in pids {
            if sh.jobs.find_pid(pid).is_none() {
                status = 127;
                continue;
            }
            match wait_one(sh, pid)? {
                Ok(s) => status = s,
                Err(s) => return Ok(s),
            }
        }
    }
    sh.jobs.jobs.retain(|j| !j.done());
    Ok(status)
}
