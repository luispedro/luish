//! Simple commands: expansion, assignments, command lookup, and exec.

use std::rc::Rc;

use crate::ast::*;
use crate::builtins::{self, BuiltinFn};
use crate::cmdtext;
use crate::exec::ForkKind;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
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

    /// Prints a `set -x` trace line.
    pub fn xtrace(&mut self, assigns: &[(Vec<u8>, Vec<u8>)], argv: &[Vec<u8>]) {
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
        sys::write_all(2, &line);
    }

    pub fn run_simple(&mut self, cmd: &SimpleCommand, no_fork: bool) -> ExecResult {
        self.lineno = cmd.lineno;
        self.subst_status = None;
        let argv = self.expand_words(&cmd.words)?;
        let mut assigns = Vec::with_capacity(cmd.assigns.len());
        for a in &cmd.assigns {
            let v = self.expand_word_str(&a.value)?;
            if argv.is_empty() {
                // Assignments without a command take effect one by one.
                self.set_var(&a.name, v.clone())?;
            }
            assigns.push((a.name.clone(), v));
        }
        if self.opt(Opt::Xtrace) && (!argv.is_empty() || !assigns.is_empty()) {
            self.xtrace(&assigns, &argv);
        }
        if argv.is_empty() {
            let status = self.subst_status.unwrap_or(0);
            return match self.redirect(&cmd.redirs, true) {
                Ok(saved) => {
                    self.restore_redirs(saved);
                    Ok(status)
                }
                Err(Flow::Error(n)) => Ok(n),
                Err(e) => Err(e),
            };
        }
        match self.lookup_command(&argv[0], true) {
            CommandKind::Special(f) => {
                for (n, v) in assigns {
                    self.set_var(&n, v)?;
                }
                if argv[0] == b"exec" && argv[1..].iter().all(|a| a == b"--") {
                    // `exec` without a command: the redirections persist.
                    let _ = self.redirect(&cmd.redirs, false)?;
                    return Ok(0);
                }
                let saved = self.redirect(&cmd.redirs, true)?;
                let r = f(self, &argv);
                self.restore_redirs(saved);
                r
            }
            CommandKind::Function(body) => self.with_temp_assigns(assigns, |sh| {
                let saved = match sh.redirect(&cmd.redirs, true) {
                    Ok(s) => s,
                    Err(Flow::Error(n)) => return Ok(n),
                    Err(e) => return Err(e),
                };
                let r = sh.call_function(&body, &argv);
                sh.restore_redirs(saved);
                r
            }),
            CommandKind::Builtin(f) => self.with_temp_assigns(assigns, |sh| {
                let saved = match sh.redirect(&cmd.redirs, true) {
                    Ok(s) => s,
                    Err(Flow::Error(n)) => return Ok(n),
                    Err(e) => return Err(e),
                };
                let r = f(sh, &argv);
                sh.restore_redirs(saved);
                r
            }),
            CommandKind::External => {
                // Replace the shell process only if no trap needs it.
                let exec_now = no_fork && !self.has_traps();
                let pid = if exec_now {
                    0
                } else {
                    self.fork_child(ForkKind::Foreground(0))?
                };
                if pid == 0 {
                    if self.redirect(&cmd.redirs, false).is_err() {
                        sys::exit(2);
                    }
                    for (n, v) in assigns {
                        if self.set_var(&n, v).is_err() {
                            sys::exit(2);
                        }
                        self.vars.entry(&n).exported = true;
                    }
                    self.exec_argv(&argv);
                }
                Ok(self.wait_foreground(&[pid], || vec![cmdtext::simple(cmd)]))
            }
        }
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
                let pid = self.fork_child(ForkKind::Foreground(0))?;
                if pid == 0 {
                    self.exec_argv(argv);
                }
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
            let mut args = vec![self.arg0.clone(), path.clone()];
            args.extend_from_slice(&argv[1..]);
            e = sys::execve(&self.self_exe.clone(), &args, &env);
        }
        let (msg, code) = match e {
            libc::ENOENT | libc::ENOTDIR => ("not found".to_string(), 127),
            libc::EACCES | libc::EISDIR => ("Permission denied".to_string(), 126),
            _ => (sys::strerror(e), 126),
        };
        self.error(format!("{}: {msg}", String::from_utf8_lossy(name)));
        sys::exit(code)
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
