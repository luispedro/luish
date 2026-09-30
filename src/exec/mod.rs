//! The executor: walks the AST and runs commands.

mod cond;
mod fork;
mod not_found;
mod procsubst;
pub mod redirect;
mod simple;

pub use fork::{ForkKind, report_signaled, signal_description};
pub use procsubst::ProcSub;
#[cfg(feature = "plugins")]
pub use simple::shell_quote;
pub use simple::{AssignValue, CommandKind};

use std::rc::Rc;

use crate::ast::*;
use crate::cmdtext;
use crate::jobs::Job;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;
use redirect::RedirError;

impl Shell {
    pub fn run_list(&mut self, list: &List) -> ExecResult {
        self.run_list_exit(list, false)
    }

    /// Runs a list. `exit` means that the shell exits right after it (dash's
    /// `EV_EXIT`), so its last command may replace the shell process.
    pub fn run_list_exit(&mut self, list: &List, exit: bool) -> ExecResult {
        // Every function, `eval`, `.`, trap and nested command comes here.
        self.check_stack()?;
        let mut status = 0;
        for (i, cc) in list.iter().enumerate() {
            status = self.run_complete(cc, exit && i + 1 == list.len())?;
            self.last_status = status;
            self.run_pending_traps()?;
        }
        Ok(status)
    }

    fn run_complete(&mut self, cc: &CompleteCommand, exit: bool) -> ExecResult {
        if !cc.async_ {
            return self.run_and_or(&cc.list, exit);
        }
        let ao = &cc.list;
        if ao.rest.is_empty() && !ao.first.negated && ao.first.cmds.len() > 1 {
            // Fork the processes of a pipeline directly, as dash does, so
            // that `$!` is the last one.
            return self.run_multi_pipeline(&ao.first.cmds, true);
        }
        let pid = self.fork_child(ForkKind::Background(0))?;
        if pid == 0 {
            let r = self.run_and_or(ao, true);
            self.child_exit(r);
        }
        self.last_bg_pid = Some(pid);
        let jobctl = self.jobctl();
        let cmd = if jobctl { cmdtext::and_or(ao) } else { String::new() };
        self.jobs.add(Job::new(vec![(pid, cmd)], jobctl, false), true);
        Ok(0)
    }

    fn run_and_or(&mut self, ao: &AndOrList, exit: bool) -> ExecResult {
        let has_rest = !ao.rest.is_empty();
        if has_rest {
            self.errexit_suppressed += 1;
        }
        let r = self.run_pipeline(&ao.first, exit && !has_rest);
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
            let r = self.run_pipeline(p, exit && last);
            if !last {
                self.errexit_suppressed -= 1;
            }
            status = r?;
        }
        Ok(status)
    }

    fn run_pipeline(&mut self, p: &Pipeline, exit: bool) -> ExecResult {
        if p.negated {
            self.errexit_suppressed += 1;
        }
        let r = if p.cmds.len() == 1 {
            self.run_command(&p.cmds[0], exit && !p.negated)
        } else {
            self.run_multi_pipeline(&p.cmds, false)
        };
        if p.negated {
            self.errexit_suppressed -= 1;
        }
        let status = r?;
        // As in zsh, `pipestatus` is left as it was by assignments (so that
        // it survives `s=$?`) and `[[`. A pipeline of several commands has
        // set it already, when it was waited for.
        if p.cmds.len() == 1
            && !matches!(
                &p.cmds[0],
                Command::Simple(sc) if sc.words.is_empty() && sc.redirs.is_empty()
            )
            && !matches!(p.cmds[0], Command::Compound(CompoundCommand::Cond { .. }, _))
        {
            self.pipestatus.clear();
            self.pipestatus.push(status);
        }
        if p.negated {
            return Ok((status == 0) as i32);
        }
        self.last_status = status;
        // As in dash, only simple commands, subshells and pipelines (and
        // `[[`, as in zsh) exit on their own status; a compound command's
        // status comes from commands inside it that have been checked
        // already (or were exempt, as on the left of `&&`).
        if p.cmds.len() > 1
            || matches!(
                p.cmds[0],
                Command::Simple(_) | Command::Compound(CompoundCommand::Subshell(_) | CompoundCommand::Cond { .. }, _)
            )
        {
            self.check_errexit(status)?;
        }
        Ok(status)
    }

    pub fn check_errexit(&self, status: i32) -> Result<(), Flow> {
        if status != 0 && self.errexit_suppressed == 0 && self.opt(Opt::Errexit) {
            return Err(Flow::Exit(status));
        }
        Ok(())
    }

    /// Runs a pipeline of two or more commands, each in its own process.
    fn run_multi_pipeline(&mut self, cmds: &[Command], background: bool) -> ExecResult {
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
            let pgid = pids.first().copied().unwrap_or(0);
            let kind = if background {
                ForkKind::Background(pgid)
            } else {
                ForkKind::Foreground(pgid)
            };
            self.remember_command(cmd);
            let pid = self.fork_child(kind)?;
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
        if background {
            self.last_bg_pid = pids.last().copied();
            let jobctl = self.jobctl();
            let procs = pids
                .into_iter()
                .zip(cmds)
                .map(|(pid, c)| (pid, if jobctl { cmdtext::command(c) } else { String::new() }))
                .collect();
            self.jobs.add(Job::new(procs, jobctl, self.opt(Opt::Pipefail)), true);
            return Ok(0);
        }
        Ok(self.wait_foreground(&pids, || cmds.iter().map(cmdtext::command).collect()))
    }

    /// Runs one command. `no_fork` means we are already in a child process
    /// that will exit afterwards, so an external command can be exec'd
    /// directly.
    pub fn run_command(&mut self, cmd: &Command, no_fork: bool) -> ExecResult {
        let mark = self.procsubs.len();
        let r = self.run_command_inner(cmd, no_fork);
        if self.procsubs.len() > mark {
            self.end_procsubs(mark);
        }
        r
    }

    fn run_command_inner(&mut self, cmd: &Command, no_fork: bool) -> ExecResult {
        match cmd {
            Command::Simple(sc) => self.run_simple(sc, no_fork),
            Command::Compound(cc, redirs) => {
                if let CompoundCommand::Subshell(list) = cc
                    && no_fork
                {
                    let saved = self.redirect(redirs, false)?;
                    let r = self.run_list_exit(list, true);
                    drop(saved);
                    return r;
                }
                let saved = match self.redirect(redirs, true) {
                    Ok(s) => s,
                    Err(RedirError::Open(n)) => {
                        self.check_errexit(n)?;
                        return Ok(n);
                    }
                    Err(e) => return Err(e.into()),
                };
                // As in dash, redirections rule out exec'ing the last command.
                let r = self.run_compound(cc, no_fork && redirs.is_empty());
                self.restore_redirs(saved);
                r
            }
            Command::FunctionDef { names, body } => {
                for name in names {
                    self.functions.insert(name.clone(), Rc::clone(body));
                }
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

    /// Runs a compound command. `exit` is as for `run_list_exit`.
    pub fn run_compound(&mut self, cc: &CompoundCommand, exit: bool) -> ExecResult {
        match cc {
            CompoundCommand::BraceGroup(list) => self.run_list_exit(list, exit),
            CompoundCommand::Subshell(list) => {
                if exit && !self.has_traps() {
                    return self.run_list_exit(list, true);
                }
                let pid = self.fork_child(ForkKind::Foreground(0))?;
                if pid == 0 {
                    let r = self.run_list_exit(list, true);
                    self.child_exit(r);
                }
                Ok(self.wait_foreground(&[pid], || vec![cmdtext::compound(cc)]))
            }
            CompoundCommand::If { conds, else_ } => {
                for (cond, body) in conds {
                    if self.run_condition(cond)? == 0 {
                        return self.run_list_exit(body, exit);
                    }
                }
                match else_ {
                    Some(body) => self.run_list_exit(body, exit),
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
                        let matched = match pat.as_literal() {
                            // A pattern without special characters, as in
                            // most arms, needs no expansion or compiling.
                            Some(lit) if !lit.iter().any(|c| matches!(c, b'*' | b'?' | b'[' | b'\\')) => lit == subject,
                            _ => {
                                let p = self.expand_pattern(pat)?;
                                crate::expand::pattern::Pattern::new(&p).matches(&subject)
                            }
                        };
                        if matched {
                            return self.run_list_exit(&arm.body, exit);
                        }
                    }
                }
                Ok(0)
            }
            CompoundCommand::Cond { expr, lineno } => self.run_cond(expr, *lineno),
        }
    }

    /// Fails (as a shell error) if the stack is nearly used up: nesting
    /// that deep would otherwise crash the shell (`stack.rs`).
    #[inline]
    pub fn check_stack(&self) -> Result<(), Flow> {
        if crate::stack::ok() {
            return Ok(());
        }
        self.error(crate::stack::TOO_DEEP);
        Err(Flow::Error(2))
    }

    /// Calls a shell function with the given arguments (`argv[0]` is the
    /// function name).
    pub fn call_function(&mut self, body: &FunctionBody, argv: &[Vec<u8>]) -> ExecResult {
        if self.func_depth >= crate::stack::MAX_FUNC_DEPTH {
            let max = crate::stack::MAX_FUNC_DEPTH;
            self.error(format!("Maximum function recursion depth ({max}) reached"));
            return Err(Flow::Error(2));
        }
        let saved_pos = std::mem::replace(&mut self.positional, argv[1..].to_vec());
        let saved_getopts = (self.optind, self.optoff);
        self.reset_getopts();
        let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
        self.func_depth += 1;
        self.locals.push(Vec::new());
        let r = match self.redirect(&body.redirs, true) {
            Ok(saved) => {
                let r = self.run_compound(&body.cmd, false);
                self.restore_redirs(saved);
                r
            }
            Err(RedirError::Open(n)) => Ok(n),
            Err(e) => Err(e.into()),
        };
        for (name, saved) in self.locals.pop().unwrap().into_iter().rev() {
            self.restore_saved(name, saved);
        }
        self.func_depth -= 1;
        self.loop_depth = saved_loop;
        self.positional = saved_pos;
        (self.optind, self.optoff) = saved_getopts;
        match r {
            Err(Flow::Return(n)) => Ok(n),
            r => r,
        }
    }
}
