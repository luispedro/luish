//! The executor: walks the AST and runs commands.

mod fork;
pub mod redirect;
mod simple;

pub use fork::report_signaled;
pub use simple::CommandKind;

use std::rc::Rc;

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::signals::{self, Disposition};
use crate::sys;

impl Shell {
    pub fn run_list(&mut self, list: &List) -> ExecResult {
        let mut status = 0;
        for cc in list {
            status = self.run_complete(cc)?;
            self.last_status = status;
            self.run_pending_traps()?;
        }
        Ok(status)
    }

    fn run_complete(&mut self, cc: &CompleteCommand) -> ExecResult {
        if !cc.async_ {
            return self.run_and_or(&cc.list);
        }
        let pid = self.fork_or_error()?;
        if pid == 0 {
            if !self.opt(Opt::Monitor) {
                for sig in [libc::SIGINT, libc::SIGQUIT] {
                    if self.traps[sig as usize].is_none() {
                        signals::set_disposition(sig, Disposition::Ignore);
                    }
                }
                if let Ok(fd) = sys::open(b"/dev/null", libc::O_RDONLY, 0) {
                    let _ = sys::dup2(fd, 0);
                    sys::close(fd);
                }
            }
            let r = self.run_and_or(&cc.list);
            self.child_exit(r);
        }
        self.last_bg_pid = Some(pid);
        self.jobs.add(vec![pid], String::new());
        Ok(0)
    }

    fn run_and_or(&mut self, ao: &AndOrList) -> ExecResult {
        let has_rest = !ao.rest.is_empty();
        if has_rest {
            self.errexit_suppressed += 1;
        }
        let r = self.run_pipeline(&ao.first);
        if has_rest {
            self.errexit_suppressed -= 1;
        }
        let mut status = r?;
        for (i, (kind, p)) in ao.rest.iter().enumerate() {
            let run = match kind {
                AndOr::And => status == 0,
                AndOr::Or => status != 0,
            };
            if !run {
                continue;
            }
            self.last_status = status;
            let last = i + 1 == ao.rest.len();
            if !last {
                self.errexit_suppressed += 1;
            }
            let r = self.run_pipeline(p);
            if !last {
                self.errexit_suppressed -= 1;
            }
            status = r?;
        }
        Ok(status)
    }

    fn run_pipeline(&mut self, p: &Pipeline) -> ExecResult {
        if p.negated {
            self.errexit_suppressed += 1;
        }
        let r = if p.cmds.len() == 1 {
            self.run_command(&p.cmds[0], false)
        } else {
            self.run_multi_pipeline(&p.cmds)
        };
        if p.negated {
            self.errexit_suppressed -= 1;
        }
        let status = r?;
        if p.negated {
            return Ok((status == 0) as i32);
        }
        self.last_status = status;
        self.check_errexit(status)?;
        Ok(status)
    }

    pub fn check_errexit(&self, status: i32) -> Result<(), Flow> {
        if status != 0 && self.errexit_suppressed == 0 && self.opt(Opt::Errexit) {
            return Err(Flow::Exit(status));
        }
        Ok(())
    }

    fn run_multi_pipeline(&mut self, cmds: &[Command]) -> ExecResult {
        let mut pids = Vec::with_capacity(cmds.len());
        let mut prev_read: Option<i32> = None;
        for (i, cmd) in cmds.iter().enumerate() {
            let last = i + 1 == cmds.len();
            let pipe = if last {
                None
            } else {
                match sys::pipe() {
                    Ok(p) => Some(p),
                    Err(e) => {
                        if let Some(r) = prev_read {
                            sys::close(r);
                        }
                        self.error(format!("Pipe call failed: {}", sys::strerror(e)));
                        return Err(Flow::Error(2));
                    }
                }
            };
            let pid = self.fork_or_error()?;
            if pid == 0 {
                if let Some(r) = prev_read {
                    let _ = sys::dup2(r, 0);
                    sys::close(r);
                }
                if let Some((r, w)) = pipe {
                    sys::close(r);
                    let _ = sys::dup2(w, 1);
                    sys::close(w);
                }
                let res = self.run_command(cmd, true);
                self.child_exit(res);
            }
            if let Some(r) = prev_read {
                sys::close(r);
            }
            if let Some((r, w)) = pipe {
                sys::close(w);
                prev_read = Some(r);
            }
            pids.push(pid);
        }
        let mut status = 0;
        for pid in pids {
            status = self.wait_for(pid);
        }
        Ok(status)
    }

    /// Runs one command. `no_fork` means we are already in a child process
    /// that will exit afterwards, so an external command can be exec'd
    /// directly.
    pub fn run_command(&mut self, cmd: &Command, no_fork: bool) -> ExecResult {
        match cmd {
            Command::Simple(sc) => self.run_simple(sc, no_fork),
            Command::Compound(cc, redirs) => {
                if let CompoundCommand::Subshell(list) = cc
                    && no_fork
                {
                    let saved = self.redirect(redirs, false)?;
                    let r = self.run_list(list);
                    drop(saved);
                    return r;
                }
                let saved = match self.redirect(redirs, true) {
                    Ok(s) => s,
                    Err(Flow::Error(n)) => return Ok(n),
                    Err(e) => return Err(e),
                };
                let r = self.run_compound(cc);
                self.restore_redirs(saved);
                r
            }
            Command::FunctionDef { name, body } => {
                self.functions.insert(name.clone(), Rc::clone(body));
                Ok(0)
            }
        }
    }

    /// Runs a list with `set -e` suppressed (conditions of `if`, `while`).
    fn run_condition(&mut self, list: &List) -> ExecResult {
        self.errexit_suppressed += 1;
        let r = self.run_list(list);
        self.errexit_suppressed -= 1;
        r
    }

    pub fn run_compound(&mut self, cc: &CompoundCommand) -> ExecResult {
        match cc {
            CompoundCommand::BraceGroup(list) => self.run_list(list),
            CompoundCommand::Subshell(list) => {
                let pid = self.fork_or_error()?;
                if pid == 0 {
                    let r = self.run_list(list);
                    self.child_exit(r);
                }
                Ok(self.wait_for(pid))
            }
            CompoundCommand::If { conds, else_ } => {
                for (cond, body) in conds {
                    if self.run_condition(cond)? == 0 {
                        return self.run_list(body);
                    }
                }
                match else_ {
                    Some(body) => self.run_list(body),
                    None => Ok(0),
                }
            }
            CompoundCommand::While { cond, body, until } => {
                let mut status = 0;
                self.loop_depth += 1;
                let r = loop {
                    let c = match self.run_condition(cond) {
                        Ok(c) => c,
                        Err(Flow::Break(n)) => {
                            if n > 1 {
                                break Err(Flow::Break(n - 1));
                            }
                            break Ok(status);
                        }
                        Err(Flow::Continue(n)) => {
                            if n > 1 {
                                break Err(Flow::Continue(n - 1));
                            }
                            continue;
                        }
                        Err(e) => break Err(e),
                    };
                    if (c == 0) == *until {
                        break Ok(status);
                    }
                    match self.run_list(body) {
                        Ok(s) => status = s,
                        Err(Flow::Break(n)) => {
                            if n > 1 {
                                break Err(Flow::Break(n - 1));
                            }
                            break Ok(0);
                        }
                        Err(Flow::Continue(n)) => {
                            if n > 1 {
                                break Err(Flow::Continue(n - 1));
                            }
                            status = 0;
                        }
                        Err(e) => break Err(e),
                    }
                };
                self.loop_depth -= 1;
                r
            }
            CompoundCommand::For {
                var,
                words,
                body,
                lineno,
            } => {
                self.lineno = *lineno;
                let items = match words {
                    Some(ws) => self.expand_words(ws)?,
                    None => self.positional.clone(),
                };
                let mut status = 0;
                self.loop_depth += 1;
                let mut r = Ok(0);
                for item in items {
                    if let Err(e) = self.set_var(var, item) {
                        r = Err(e);
                        break;
                    }
                    match self.run_list(body) {
                        Ok(s) => status = s,
                        Err(Flow::Break(n)) => {
                            if n > 1 {
                                r = Err(Flow::Break(n - 1));
                            }
                            break;
                        }
                        Err(Flow::Continue(n)) => {
                            if n > 1 {
                                r = Err(Flow::Continue(n - 1));
                                break;
                            }
                        }
                        Err(e) => {
                            r = Err(e);
                            break;
                        }
                    }
                }
                self.loop_depth -= 1;
                r.map(|_| status)
            }
            CompoundCommand::Case { word, arms, lineno } => {
                self.lineno = *lineno;
                let subject = self.expand_word_str(word)?;
                for arm in arms {
                    for pat in &arm.patterns {
                        let p = self.expand_pattern(pat)?;
                        if crate::expand::pattern::Pattern::new(&p).matches(&subject) {
                            return self.run_list(&arm.body);
                        }
                    }
                }
                Ok(0)
            }
        }
    }

    /// Calls a shell function with the given arguments (`argv[0]` is the
    /// function name).
    pub fn call_function(&mut self, body: &FunctionBody, argv: &[Vec<u8>]) -> ExecResult {
        let saved_pos = std::mem::replace(&mut self.positional, argv[1..].to_vec());
        let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
        self.func_depth += 1;
        self.locals.push(Vec::new());
        let r = match self.redirect(&body.redirs, true) {
            Ok(saved) => {
                let r = self.run_compound(&body.cmd);
                self.restore_redirs(saved);
                r
            }
            Err(Flow::Error(n)) => Ok(n),
            Err(e) => Err(e),
        };
        for (name, var) in self.locals.pop().unwrap().into_iter().rev() {
            self.vars.restore(&name, var);
            self.var_changed(&name);
        }
        self.func_depth -= 1;
        self.loop_depth = saved_loop;
        self.positional = saved_pos;
        match r {
            Err(Flow::Return(n)) => Ok(n),
            r => r,
        }
    }
}
