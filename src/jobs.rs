//! Background jobs. (Full job control with process groups is Phase 10.)

use crate::sys::{self, WaitStatus};

#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    /// Process ids with their final `$?`-style status once they've exited.
    pub procs: Vec<(i32, Option<i32>)>,
    pub cmd: String,
}

impl Job {
    pub fn done(&self) -> bool {
        self.procs.iter().all(|p| p.1.is_some())
    }

    /// Status of the job: that of its last process.
    pub fn status(&self) -> Option<i32> {
        self.procs.last().and_then(|p| p.1)
    }
}

#[derive(Debug, Default)]
pub struct JobTable {
    pub jobs: Vec<Job>,
}

impl JobTable {
    pub fn add(&mut self, procs: Vec<i32>, cmd: String) -> usize {
        let id = self.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
        self.jobs.push(Job {
            id,
            procs: procs.into_iter().map(|p| (p, None)).collect(),
            cmd,
        });
        id
    }

    /// Records the status of a reaped child. Returns false if it isn't ours.
    pub fn record(&mut self, pid: i32, ws: &WaitStatus) -> bool {
        for j in &mut self.jobs {
            for p in &mut j.procs {
                if p.0 == pid {
                    if !matches!(ws, WaitStatus::Stopped(_) | WaitStatus::Continued) {
                        p.1 = Some(ws.code());
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Reaps any finished background children without blocking.
    pub fn reap(&mut self) {
        while let Ok(Some((pid, ws))) = sys::waitpid(-1, libc::WNOHANG) {
            self.record(pid, &ws);
        }
    }

    pub fn find_pid(&self, pid: i32) -> Option<usize> {
        self.jobs.iter().position(|j| j.procs.iter().any(|p| p.0 == pid))
    }

    pub fn clear(&mut self) {
        self.jobs.clear();
    }
}
