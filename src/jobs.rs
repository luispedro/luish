//! The job table and job control.
//!
//! This follows dash's model. Jobs have numbers (their slot in the table)
//! and are also kept in "current job" order: the current job (`%+`) first,
//! then the previous job (`%-`), and so on. A finished job stays in the
//! table until it has been reported (by `jobs` or a notification), or,
//! without job control, until `wait` has returned its status and a new job
//! needs a slot.
//!
//! Without job control, only background jobs are recorded, and foreground
//! commands are waited for directly. With job control (`set -m`), every job
//! gets its own process group and is recorded while it runs, so that it can
//! be stopped and continued.

use crate::exec::signal_description;
use crate::shell::{Flow, Shell};
use crate::signals;
use crate::sys::{self, WaitStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: i32,
    /// `None` while the process runs.
    pub status: Option<WaitStatus>,
    /// Command text, recorded only under job control (as in dash).
    pub cmd: String,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub procs: Vec<Proc>,
    pub state: JobState,
    /// Status of the process that stopped the job.
    pub stop_status: Option<WaitStatus>,
    /// The state changed since the job was last reported.
    pub changed: bool,
    /// `wait` has returned the job's status.
    pub waited: bool,
    /// Created under job control: the job has its own process group, led by
    /// its first process.
    pub jobctl: bool,
    /// `set -o pipefail` was on when the job started, so its status is that
    /// of the last process that failed.
    pub pipefail: bool,
}

impl Job {
    pub fn new(procs: Vec<(i32, String)>, jobctl: bool, pipefail: bool) -> Job {
        Job {
            procs: procs
                .into_iter()
                .map(|(pid, cmd)| Proc { pid, status: None, cmd })
                .collect(),
            state: JobState::Running,
            stop_status: None,
            changed: false,
            waited: false,
            jobctl,
            pipefail,
        }
    }

    pub fn pgid(&self) -> i32 {
        self.procs[0].pid
    }

    /// The job's `$?`: that of its last process, or with `pipefail` that of
    /// the last process that failed (0 if none did).
    pub fn status(&self) -> i32 {
        let code = |p: &Proc| p.status.map_or(0, |s| s.code());
        if self.pipefail {
            return self.procs.iter().rev().map(code).find(|&c| c != 0).unwrap_or(0);
        }
        self.procs.last().map_or(0, code)
    }

    /// Recomputes the state from the processes' statuses. A job runs while
    /// any process runs, and is stopped if any process is stopped.
    fn update_state(&mut self) -> JobState {
        let mut state = JobState::Done;
        for p in &self.procs {
            match p.status {
                None => return JobState::Running,
                Some(s @ WaitStatus::Stopped(_)) => {
                    self.stop_status = Some(s);
                    state = JobState::Stopped;
                }
                Some(_) => {}
            }
        }
        state
    }

    /// The command text of the whole job.
    pub fn text(&self) -> String {
        let mut s = self.procs[0].cmd.clone();
        for p in &self.procs[1..] {
            s.push_str(" | ");
            s.push_str(&p.cmd);
        }
        s
    }
}

#[derive(Debug, Default)]
pub struct JobTable {
    /// Job number `n` is in slot `n - 1`.
    slots: Vec<Option<Job>>,
    /// Slot indices, from the current job to the least recent one.
    order: Vec<usize>,
}

impl JobTable {
    pub fn get(&self, i: usize) -> &Job {
        self.slots[i].as_ref().expect("job slot in use")
    }

    pub fn get_mut(&mut self, i: usize) -> &mut Job {
        self.slots[i].as_mut().expect("job slot in use")
    }

    /// Slot indices in current-job order.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    pub fn number(i: usize) -> usize {
        i + 1
    }

    /// The slot of job number `n`, if it is in use.
    pub fn by_number(&self, n: usize) -> Option<usize> {
        (n > 0 && self.slots.get(n - 1).is_some_and(|s| s.is_some())).then(|| n - 1)
    }

    /// Frees the first finished job that `wait` has already reported, if no
    /// free slot comes before it. dash does this whenever it creates a job
    /// without job control, so it limits how long such jobs stay visible.
    pub fn reclaim(&mut self, jobctl: bool) {
        if jobctl {
            return;
        }
        for i in 0..self.slots.len() {
            match &self.slots[i] {
                None => return,
                Some(j) if j.state == JobState::Done && j.waited => {
                    self.free(i);
                    return;
                }
                Some(_) => {}
            }
        }
    }

    /// Adds a job in the lowest free slot. A background job goes after the
    /// stopped jobs in current-job order; any other job becomes current.
    pub fn add(&mut self, job: Job, background: bool) -> usize {
        let i = match self.slots.iter().position(|s| s.is_none()) {
            Some(i) => i,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.slots[i] = Some(job);
        self.order.insert(0, i);
        if background {
            self.make_current(i, false);
        }
        i
    }

    pub fn free(&mut self, i: usize) {
        self.slots[i] = None;
        self.order.retain(|&j| j != i);
        while self.slots.last().is_some_and(|s| s.is_none()) {
            self.slots.pop();
        }
    }

    /// Moves a job to the front of the current-job order, or, if `stopped`
    /// is false, to just after the stopped jobs (dash's `set_curjob`).
    pub fn make_current(&mut self, i: usize, stopped: bool) {
        self.order.retain(|&j| j != i);
        let pos = if stopped {
            0
        } else {
            self.order
                .iter()
                .position(|&j| self.get(j).state != JobState::Stopped)
                .unwrap_or(self.order.len())
        };
        self.order.insert(pos, i);
    }

    /// Records the new status of a child. Returns the slot of its job, or
    /// `None` if it isn't one of ours.
    pub fn record(&mut self, pid: i32, ws: WaitStatus) -> Option<usize> {
        let i = self.order.iter().copied().find(|&i| {
            let j = self.get(i);
            j.state != JobState::Done && j.procs.iter().any(|p| p.pid == pid)
        })?;
        let job = self.get_mut(i);
        for p in &mut job.procs {
            if p.pid == pid {
                p.status = Some(ws);
            }
        }
        let state = job.update_state();
        if state != JobState::Running {
            job.changed = true;
            if job.state != state {
                job.state = state;
                if state == JobState::Stopped {
                    self.make_current(i, true);
                }
            }
        }
        Some(i)
    }

    /// The slot of the job whose last process is `pid` (as `wait pid`
    /// looks it up).
    pub fn find_last_pid(&self, pid: i32) -> Option<usize> {
        self.order
            .iter()
            .copied()
            .find(|&i| self.get(i).procs.last().is_some_and(|p| p.pid == pid))
    }

    pub fn find_pid(&self, pid: i32) -> Option<usize> {
        self.order
            .iter()
            .copied()
            .find(|&i| self.get(i).procs.iter().any(|p| p.pid == pid))
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.order.clear();
    }
}

/// The controlling terminal, while job control is on.
#[derive(Debug)]
pub struct Terminal {
    /// A close-on-exec fd for the terminal.
    pub fd: i32,
    /// The foreground process group when job control was turned on; it gets
    /// the terminal back when job control is turned off.
    pub initial_pgrp: i32,
    /// The shell's terminal modes, restored after a job stops or is killed
    /// by a signal.
    pub modes: Option<libc::termios>,
}

pub enum Waited {
    Child(i32, WaitStatus),
    NoChildren,
    /// A trapped signal (whose trap has run) interrupted the wait.
    Interrupted(i32),
}

/// What `jobs` shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShowMode {
    Normal,
    /// `-l`: process ids too.
    Pids,
    /// `-p`: only the process group id.
    Pgid,
}

/// A job's status as dash's `sprint_status` shows it.
pub fn status_text(ws: WaitStatus) -> String {
    match ws {
        WaitStatus::Exited(0) => "Done".into(),
        WaitStatus::Exited(n) => format!("Done({n})"),
        WaitStatus::Signaled(sig, core) => {
            let mut s: String = signal_description(sig).chars().take(32).collect();
            if core {
                s.push_str(" (core dumped)");
            }
            s
        }
        WaitStatus::Stopped(sig) => signal_description(sig).chars().take(32).collect(),
        WaitStatus::Continued => "Running".into(),
    }
}

/// Pads `s` to column 33 (always at least one space), then adds `cmd`.
fn pad_to_cmd(s: &mut String, col: usize, cmd: &str) {
    s.extend(std::iter::repeat_n(' ', 33usize.saturating_sub(col).max(1)));
    s.push_str(cmd);
}

impl Shell {
    pub fn jobctl(&self) -> bool {
        self.jobctl.is_some()
    }

    /// Turns job control on or off (dash's `setjobctl`). Only the main shell
    /// does job control.
    pub fn set_jobctl(&mut self, on: bool) {
        if on == self.jobctl() || self.in_subshell {
            return;
        }
        if !on {
            let t = self.jobctl.take().unwrap();
            let _ = sys::tcsetpgrp(t.fd, t.initial_pgrp);
            let _ = sys::setpgid(0, t.initial_pgrp);
            self.apply_trap_disposition(libc::SIGTSTP as usize);
            self.apply_trap_disposition(libc::SIGTTOU as usize);
            sys::close(t.fd);
            return;
        }
        let fd = match sys::open(b"/dev/tty", libc::O_RDWR, 0) {
            Ok(fd) => {
                let high = sys::dup_high(fd);
                sys::close(fd);
                high.ok()
            }
            Err(_) => [2, 1, 0]
                .into_iter()
                .find(|&fd| sys::isatty(fd))
                .and_then(|fd| sys::dup_high(fd).ok()),
        };
        // Wait until we are in the foreground.
        let pgrp = fd.and_then(|fd| {
            loop {
                match sys::tcgetpgrp(fd) {
                    Err(_) => break None,
                    Ok(p) if p == sys::getpgrp() => break Some(p),
                    Ok(_) => {
                        let _ = sys::kill(0, libc::SIGTTIN);
                    }
                }
            }
        });
        let (Some(fd), Some(pgrp)) = (fd, pgrp) else {
            if let Some(fd) = fd {
                sys::close(fd);
            }
            self.error("can't access tty; job control turned off");
            self.options.set(crate::options::Opt::Monitor, false);
            return;
        };
        self.jobctl = Some(Terminal {
            fd,
            initial_pgrp: pgrp,
            modes: None,
        });
        self.apply_trap_disposition(libc::SIGTSTP as usize);
        self.apply_trap_disposition(libc::SIGTTOU as usize);
        let _ = sys::setpgid(0, self.pid);
        let _ = sys::tcsetpgrp(fd, self.pid);
        self.jobctl.as_mut().unwrap().modes = sys::tcgetattr(fd);
    }

    /// Records the status of any children that changed state, without
    /// blocking.
    pub fn reap_jobs(&mut self) {
        let flags = libc::WNOHANG | if self.jobctl() { libc::WUNTRACED } else { 0 };
        while let Ok(Some((pid, ws))) = sys::waitpid(-1, flags) {
            self.jobs.record(pid, ws);
        }
    }

    /// Waits for any child to change state and records its new status. With
    /// `interruptible`, a trapped signal ends the wait after its trap runs.
    pub fn wait_any(&mut self, interruptible: bool) -> Result<Waited, Flow> {
        let flags = if self.jobctl() { libc::WUNTRACED } else { 0 };
        loop {
            match sys::waitpid(-1, flags) {
                Ok(Some((pid, ws))) => {
                    self.jobs.record(pid, ws);
                    return Ok(Waited::Child(pid, ws));
                }
                Ok(None) => continue,
                Err(libc::EINTR) => {
                    if !interruptible {
                        continue;
                    }
                    let trapped = signals::peek_pending()
                        .into_iter()
                        .rev()
                        .find(|&s| self.traps[s].as_ref().is_some_and(|a| !a.is_empty()));
                    self.run_pending_traps()?;
                    if let Some(sig) = trapped {
                        return Ok(Waited::Interrupted(sig as i32));
                    }
                }
                Err(_) => return Ok(Waited::NoChildren),
            }
        }
    }

    /// Waits for a job under job control to finish or stop, gives the
    /// terminal back to the shell, and returns the job's status (dash's
    /// `waitforjob`). A finished job is removed from the table.
    pub fn wait_job(&mut self, i: usize) -> i32 {
        while self.jobs.get(i).state == JobState::Running {
            match self.wait_any(false) {
                Ok(Waited::Child(pid, ws)) => {
                    if self.jobs.get(i).procs.iter().any(|p| p.pid == pid) {
                        crate::exec::report_signaled(&ws);
                    }
                }
                _ => {
                    // No children left: the job's processes were reaped
                    // elsewhere. Don't wait forever.
                    let job = self.jobs.get_mut(i);
                    for p in &mut job.procs {
                        p.status.get_or_insert(WaitStatus::Exited(127));
                    }
                    job.state = job.update_state();
                }
            }
        }
        let job = self.jobs.get(i);
        let status = match job.procs.last().and_then(|p| p.status) {
            Some(WaitStatus::Stopped(sig)) => 128 + sig,
            _ => job.status(),
        };
        let sigint = matches!(
            job.procs.last().and_then(|p| p.status),
            Some(WaitStatus::Signaled(libc::SIGINT, _))
        );
        let abnormal = job.state == JobState::Stopped
            || matches!(job.procs.last().and_then(|p| p.status), Some(WaitStatus::Signaled(..)));
        let done = job.state == JobState::Done;
        if job.procs.len() > 1 {
            self.pipestatus = job.procs.iter().map(|p| p.status.map_or(0, |s| s.code())).collect();
        }
        if job.jobctl
            && let Some(t) = &mut self.jobctl
        {
            let _ = sys::tcsetpgrp(t.fd, self.pid);
            // Keep the modes a program like `stty` set, but undo those of a
            // program that stopped or died without restoring them.
            if abnormal {
                if let Some(m) = &t.modes {
                    sys::tcsetattr(t.fd, m);
                }
            } else {
                t.modes = sys::tcgetattr(t.fd);
            }
            // The job had the terminal, so the shell didn't see the SIGINT
            // that killed it: act as though it did.
            if sigint {
                sys::raise(libc::SIGINT);
            }
        }
        if done {
            self.jobs.free(i);
        }
        status
    }

    /// Formats one job for `jobs` or a notification. A finished job is
    /// removed from the table once shown (dash's `showjob`).
    pub fn show_job(&mut self, i: usize, mode: ShowMode) -> String {
        let job = self.jobs.get(i);
        if mode == ShowMode::Pgid {
            return format!("{}\n", job.pgid());
        }
        let mut s = format!("[{}]   ", JobTable::number(i));
        let indent = s.len();
        let order = self.jobs.order();
        if order.first() == Some(&i) {
            s.replace_range(indent - 2..indent - 1, "+");
        } else if order.get(1) == Some(&i) {
            s.replace_range(indent - 2..indent - 1, "-");
        }
        if mode == ShowMode::Pids {
            s.push_str(&format!("{} ", job.procs[0].pid));
        }
        match job.state {
            JobState::Running => s.push_str("Running"),
            JobState::Stopped => s.push_str(&status_text(job.stop_status.unwrap_or(WaitStatus::Stopped(0)))),
            JobState::Done => s.push_str(&status_text(job.procs.last().unwrap().status.unwrap())),
        }
        let col = s.len();
        pad_to_cmd(&mut s, col, &job.procs[0].cmd);
        for p in &job.procs[1..] {
            if mode == ShowMode::Pids {
                s.push_str(" |\n");
                let start = s.len();
                s.extend(std::iter::repeat_n(' ', indent));
                s.push_str(&format!("{} ", p.pid));
                let col = s.len() - start;
                pad_to_cmd(&mut s, col, &p.cmd);
            } else {
                s.push_str(" | ");
                s.push_str(&p.cmd);
            }
        }
        s.push('\n');
        let job = self.jobs.get_mut(i);
        job.changed = false;
        if job.state == JobState::Done {
            self.jobs.free(i);
        }
        s
    }

    /// Reports the jobs whose state changed, before a prompt.
    pub fn notify_jobs(&mut self) {
        if !self.jobctl() {
            return;
        }
        self.reap_jobs();
        let mut out = String::new();
        for i in self.jobs.order().to_vec() {
            if self.jobs.get(i).changed {
                out.push_str(&self.show_job(i, ShowMode::Normal));
            }
        }
        sys::write_all(2, out.as_bytes());
    }

    /// Warns once if the current job is stopped (before `exit` or at end
    /// of input). Returns true if the shell should not exit yet.
    pub fn stopped_jobs_warning(&mut self) -> bool {
        if self.job_warning != 0 {
            return false;
        }
        match self.jobs.order().first() {
            Some(&i) if self.jobs.get(i).state == JobState::Stopped => {
                sys::write_all(2, b"You have stopped jobs.\n");
                self.job_warning = 2;
                true
            }
            _ => false,
        }
    }

    /// Resolves a job spec (dash's `getjob`). `None` means the current job.
    /// With `need_jobctl`, the job must have been created under job control.
    pub fn get_job(&self, spec: Option<&[u8]>, need_jobctl: bool) -> Result<usize, String> {
        let shown = |s: Option<&[u8]>| String::from_utf8_lossy(s.unwrap_or(b"%%")).into_owned();
        let order = self.jobs.order();
        let found = match spec {
            None => order.first().copied().ok_or_else(|| "No current job".to_string()),
            Some(s) if s.first() != Some(&b'%') => Err(format!("No such job: {}", shown(spec))),
            Some(s) => {
                let rest = &s[1..];
                match rest {
                    b"" | b"%" | b"+" => order.first().copied().ok_or_else(|| "No current job".to_string()),
                    b"-" => order.get(1).copied().ok_or_else(|| "No previous job".to_string()),
                    _ => {
                        let by_num = crate::builtins::parse_uint(rest).and_then(|n| self.jobs.by_number(n as usize));
                        match by_num {
                            Some(i) => Ok(i),
                            None => {
                                let (sub, pat) = match rest.strip_prefix(b"?") {
                                    Some(p) => (true, p),
                                    None => (false, rest),
                                };
                                let matches = |cmd: &str| {
                                    let c = cmd.as_bytes();
                                    if sub {
                                        pat.is_empty() || c.windows(pat.len()).any(|w| w == pat)
                                    } else {
                                        c.starts_with(pat)
                                    }
                                };
                                let mut found = None;
                                for &i in order {
                                    if matches(&self.jobs.get(i).procs[0].cmd) {
                                        if found.is_some() {
                                            return Err(format!("{}: ambiguous", shown(spec)));
                                        }
                                        found = Some(i);
                                    }
                                }
                                found.ok_or_else(|| format!("No such job: {}", shown(spec)))
                            }
                        }
                    }
                }
            }
        }?;
        if need_jobctl && !self.jobs.get(found).jobctl {
            return Err(format!("job {} not created under job control", shown(spec)));
        }
        Ok(found)
    }

    /// Continues a job in the foreground or the background (dash's
    /// `restartjob`). Returns the job's status for the foreground.
    pub fn restart_job(&mut self, i: usize, foreground: bool) -> i32 {
        let job = self.jobs.get_mut(i);
        if job.state != JobState::Done {
            job.state = JobState::Running;
            let pgid = job.pgid();
            for p in &mut job.procs {
                if matches!(p.status, Some(WaitStatus::Stopped(_))) {
                    p.status = None;
                }
            }
            if foreground && let Some(t) = &self.jobctl {
                let _ = sys::tcsetpgrp(t.fd, pgid);
            }
            let _ = sys::kill(-pgid, libc::SIGCONT);
        }
        if foreground { self.wait_job(i) } else { 0 }
    }
}
