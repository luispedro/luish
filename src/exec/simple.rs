//! Simple commands: expansion, assignments, command lookup, and exec.

use std::rc::Rc;

use crate::ast::*;
use crate::builtins::{self, BuiltinFn};
use crate::cmdtext;
use crate::exec::ForkKind;
use crate::exec::redirect::RedirError;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};

type EResult = Result<Vec<(Vec<u8>, Vec<u8>)>, Flow>;
use crate::sys;

pub enum CommandKind {
    Special(BuiltinFn),
    Function(Rc<FunctionBody>),
    Builtin(BuiltinFn),
    External,
}

/// Quotes a word so that it can be read back by the shell, as in `set -x`
/// output.
pub fn shell_quote(s: &[u8]) -> Vec<u8> {
    let safe = |c: &u8| c.is_ascii_alphanumeric() || b"_/.,:=+@%^-".contains(c);
    if !s.is_empty() && s.iter().all(safe) {
        return s.to_vec();
    }
    let mut out = vec![b'\''];
    for &c in s {
        if c == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(c);
        }
    }
    out.push(b'\'');
    out
}

impl Shell {
    pub fn lookup_command(&self, name: &[u8], functions: bool) -> CommandKind {
        if let Some((f, special)) = builtins::lookup(name) {
            if special {
                return CommandKind::Special(f);
            }
            if functions && let Some(body) = self.functions.get(name) {
                return CommandKind::Function(body.clone());
            }
            return CommandKind::Builtin(f);
        }
        if functions && let Some(body) = self.functions.get(name) {
            return CommandKind::Function(body.clone());
        }
        CommandKind::External
    }

    /// Prints a `set -x` trace line to `fd`.
    pub fn xtrace(&mut self, fd: i32, assigns: &[(Vec<u8>, Vec<u8>)], argv: &[Vec<u8>]) {
        let mut line = self.expand_prompt(b"PS4");
        let mut first = true;
        for (n, v) in assigns {
            if !first {
                line.push(b' ');
            }
            first = false;
            line.extend_from_slice(n);
            line.push(b'=');
            line.extend(shell_quote(v));
        }
        for a in argv {
            if !first {
                line.push(b' ');
            }
            first = false;
            line.extend(shell_quote(a));
        }
        line.push(b'\n');
        sys::write_all(fd, &line);
    }

    /// As in dash and POSIX (XCU 2.9.1): the words are expanded, then the
    /// redirections are made (in the shell, for every kind of command), then
    /// the assignments are expanded.
    pub fn run_simple(&mut self, cmd: &SimpleCommand, no_fork: bool) -> ExecResult {
        self.lineno = cmd.lineno;
        self.subst_status = None;
        let argv = self.expand_command_words(&cmd.words)?;
        let special = argv
            .first()
            .is_some_and(|a| matches!(builtins::lookup(a), Some((_, true))));
        if special && argv[0] == b"exec" && argv[1..].iter().all(|a| a == b"--") {
            // `exec` without a command: the redirections persist.
            let _ = self.redirect(&cmd.redirs, false)?;
            let assigns = self.expand_assigns(cmd, true)?;
            self.trace(2, &assigns, &argv);
            return Ok(0);
        }
        let saved = match self.redirect(&cmd.redirs, true) {
            Ok(s) => s,
            Err(RedirError::Open(n)) if !special => return Ok(n),
            Err(e) => return Err(e.into()),
        };
        let r = self.run_simple_redirected(cmd, argv, no_fork, saved.original(2).unwrap_or(-1));
        self.restore_redirs(saved);
        r
    }

    /// Expands the assignments of `cmd`. With `now` (no command, or a
    /// special built-in), each takes effect before the next is expanded.
    fn expand_assigns(&mut self, cmd: &SimpleCommand, now: bool) -> EResult {
        let mut assigns = Vec::with_capacity(cmd.assigns.len());
        for a in &cmd.assigns {
            let v = self.expand_word_str(&a.value)?;
            if now {
                self.set_var(&a.name, v.clone())?;
            }
            assigns.push((a.name.clone(), v));
        }
        Ok(assigns)
    }

    fn trace(&mut self, fd: i32, assigns: &[(Vec<u8>, Vec<u8>)], argv: &[Vec<u8>]) {
        if self.opt(Opt::Xtrace) && (!argv.is_empty() || !assigns.is_empty()) && fd >= 0 {
            self.xtrace(fd, assigns, argv);
        }
    }

    fn run_simple_redirected(
        &mut self,
        cmd: &SimpleCommand,
        argv: Vec<Vec<u8>>,
        no_fork: bool,
        err_fd: i32,
    ) -> ExecResult {
        if argv.is_empty() {
            let assigns = self.expand_assigns(cmd, true)?;
            self.trace(err_fd, &assigns, &argv);
            return Ok(self.subst_status.unwrap_or(0));
        }
        let kind = self.lookup_command(&argv[0], true);
        let special = matches!(kind, CommandKind::Special(_));
        let assigns = self.expand_assigns(cmd, special)?;
        self.trace(err_fd, &assigns, &argv);
        if special && argv[0] == b"exec" {
            // As in dash, `exec` exports its assignments to the command.
            for (n, _) in &assigns {
                self.vars.entry(n).exported = true;
            }
        }
        match kind {
            CommandKind::Special(f) => f(self, &argv),
            CommandKind::Function(body) => self.with_temp_assigns(assigns, |sh| sh.call_function(&body, &argv)),
            CommandKind::Builtin(f) => self.with_temp_assigns(assigns, |sh| f(sh, &argv)),
            // As in dash, the assignments are made (temporarily) in the shell,
            // so that an error in one is the shell's.
            CommandKind::External => self.with_temp_assigns(assigns, |sh| sh.run_external(cmd, &argv, no_fork)),
        }
    }

    fn run_external(&mut self, cmd: &SimpleCommand, argv: &[Vec<u8>], no_fork: bool) -> ExecResult {
        // Replace the shell process only if no trap needs it.
        let exec_now = no_fork && !self.has_traps();
        if !exec_now && self.can_spawn() {
            // Like dash's `vforkexec`.
            return Ok(match self.spawn_argv(argv) {
                Ok(pid) => self.wait_foreground(&[pid], || vec![cmdtext::simple(cmd)]),
                Err(status) => status,
            });
        }
        let pid = if exec_now {
            0
        } else {
            self.fork_child(ForkKind::Foreground(0))?
        };
        if pid == 0 {
            self.exec_argv(argv);
        }
        Ok(self.wait_foreground(&[pid], || vec![cmdtext::simple(cmd)]))
    }

    /// Whether an external command can be started with `spawn` instead of
    /// `fork`. The child of `spawn` runs no shell code, so this needs a
    /// shell whose children need no set-up: no job control (the child would
    /// have to take the terminal), and not interactive (the child would have
    /// to reset the signals the shell ignores).
    fn can_spawn(&self) -> bool {
        !self.interactive && self.jobctl.is_none()
    }

    /// Starts an external command without forking the shell (see
    /// `can_spawn`). Returns its pid, or the status if it couldn't be run.
    fn spawn_argv(&mut self, argv: &[Vec<u8>]) -> Result<i32, i32> {
        let name = &argv[0];
        let path = if name.contains(&b'/') {
            name.clone()
        } else {
            match self.find_in_path(name) {
                Some(p) => p,
                None => {
                    self.error(format!("{}: not found", String::from_utf8_lossy(name)));
                    return Err(127);
                }
            }
        };
        // A new job: dash frees a finished job's slot at this point.
        self.jobs.reclaim(false);
        let env = self.vars.environ();
        let mut r = sys::spawn(&path, argv, &env);
        if r == Err(libc::ENOEXEC) {
            // A script without `#!`: run it with this shell.
            r = sys::spawn(&self.self_exe, &self.script_args(&path, argv), &env);
        }
        r.map_err(|e| self.exec_error(name, e))
    }

    /// Runs `f` with variable assignments that only last for its duration.
    fn with_temp_assigns(
        &mut self,
        assigns: Vec<(Vec<u8>, Vec<u8>)>,
        f: impl FnOnce(&mut Shell) -> ExecResult,
    ) -> ExecResult {
        let mut saved = Vec::with_capacity(assigns.len());
        let mut r = None;
        for (n, v) in assigns {
            saved.push((n.clone(), self.vars.take(&n)));
            if let Err(e) = self.set_var(&n, v) {
                r = Some(Err(e));
                break;
            }
            self.vars.entry(&n).exported = true;
        }
        let r = r.unwrap_or_else(|| f(self));
        for (n, old) in saved.into_iter().rev() {
            self.vars.restore(&n, old);
            self.var_changed(&n);
        }
        r
    }

    /// Runs a command given as expanded words, without assignments or
    /// redirections (for `command`).
    pub fn run_argv(&mut self, argv: &[Vec<u8>], functions: bool) -> ExecResult {
        match self.lookup_command(&argv[0], functions) {
            CommandKind::Special(f) | CommandKind::Builtin(f) => f(self, argv),
            CommandKind::Function(body) => self.call_function(&body, argv),
            CommandKind::External => {
                let pid = if self.can_spawn() {
                    match self.spawn_argv(argv) {
                        Ok(pid) => pid,
                        Err(status) => return Ok(status),
                    }
                } else {
                    let pid = self.fork_child(ForkKind::Foreground(0))?;
                    if pid == 0 {
                        self.exec_argv(argv);
                    }
                    pid
                };
                let text = || vec![String::from_utf8_lossy(&argv.join(&b' ')).into_owned()];
                Ok(self.wait_foreground(&[pid], text))
            }
        }
    }

    /// Replaces the process with an external command. Never returns.
    pub fn exec_argv(&mut self, argv: &[Vec<u8>]) -> ! {
        let name = &argv[0];
        let path = if name.contains(&b'/') {
            Some(name.clone())
        } else {
            self.find_in_path(name)
        };
        let Some(path) = path else {
            self.error(format!("{}: not found", String::from_utf8_lossy(name)));
            sys::exit(127);
        };
        let env = self.vars.environ();
        let mut e = sys::execve(&path, argv, &env);
        if e == libc::ENOEXEC {
            // A script without `#!`: run it with this shell.
            e = sys::execve(&self.self_exe, &self.script_args(&path, argv), &env);
        }
        let code = self.exec_error(name, e);
        sys::exit(code)
    }

    /// The arguments for running `path`, a script without `#!`, with this
    /// shell.
    fn script_args(&self, path: &[u8], argv: &[Vec<u8>]) -> Vec<Vec<u8>> {
        let mut args = vec![self.arg0.clone(), path.to_vec()];
        args.extend_from_slice(&argv[1..]);
        args
    }

    /// Reports that `name` couldn't be executed (errno `e`), and returns
    /// the status for it.
    fn exec_error(&self, name: &[u8], e: i32) -> i32 {
        let (msg, code) = match e {
            libc::ENOENT | libc::ENOTDIR => ("not found".to_string(), 127),
            libc::EACCES | libc::EISDIR => ("Permission denied".to_string(), 126),
            _ => (sys::strerror(e), 126),
        };
        self.error(format!("{}: {msg}", String::from_utf8_lossy(name)));
        code
    }

    /// Parameter-expands a prompt variable (`PS1`, `PS2`, `PS4`).
    pub fn expand_prompt(&mut self, var: &[u8]) -> Vec<u8> {
        let text = self.get_var(var).unwrap_or_default();
        if !text.contains(&b'$') && !text.contains(&b'`') && !text.contains(&b'\\') {
            return text;
        }
        let saved_status = self.last_status;
        let saved_lineno = self.lineno;
        let r = match crate::lexer::parse_string_word(&text) {
            Ok(w) => self.expand_word_str(&w).unwrap_or(text),
            Err(_) => text,
        };
        self.last_status = saved_status;
        self.lineno = saved_lineno;
        r
    }
}
