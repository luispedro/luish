//! Simple commands: expansion, assignments, command lookup, and exec.

use std::rc::Rc;

use crate::ast::*;
use crate::builtins::{self, BuiltinFn};
use crate::cmdtext;
use crate::exec::ForkKind;
use crate::exec::redirect::RedirError;
use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};

type EResult = Result<Vec<Assignment>, Flow>;
use crate::sys;
use crate::vars::Value;

/// An assignment after expansion.
#[derive(Clone)]
pub struct Assignment {
    pub name: Vec<u8>,
    /// The element assigned to, `name[index]=`.
    pub index: Option<i64>,
    /// `name+=value`.
    pub append: bool,
    pub value: Value,
}

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
        if let Some((f, special)) = self.builtin(name) {
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
    pub fn xtrace(&mut self, fd: i32, assigns: &[Assignment], argv: &[Vec<u8>]) {
        let mut line = self.expand_prompt(b"PS4");
        let mut first = true;
        for a in assigns {
            if !first {
                line.push(b' ');
            }
            first = false;
            line.extend_from_slice(&a.name);
            if let Some(i) = a.index {
                line.extend_from_slice(format!("[{i}]").as_bytes());
            }
            line.extend_from_slice(if a.append { b"+=" } else { b"=" });
            match &a.value {
                Value::Str(v) => line.extend(shell_quote(v)),
                // As zsh shows it.
                Value::Array(items) => {
                    line.extend_from_slice(b"( ");
                    for v in items.iter() {
                        line.extend(shell_quote(v));
                        line.push(b' ');
                    }
                    line.push(b')');
                }
            }
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
    /// special built-in), each takes effect before the next is expanded,
    /// and they are only returned for `set -x`.
    fn expand_assigns(&mut self, cmd: &SimpleCommand, now: bool) -> EResult {
        let keep = !now || self.opt(Opt::Xtrace);
        let mut assigns = Vec::with_capacity(if keep { cmd.assigns.len() } else { 0 });
        for a in &cmd.assigns {
            // The common case, `name=value`.
            if now && !keep && a.index.is_none() && !a.append && a.array().is_none() {
                let v = self.expand_word_str(&a.value)?;
                self.set_var(&a.name, v)?;
                continue;
            }
            let x = self.expand_assign(a)?;
            if !now {
                assigns.push(x);
            } else if keep {
                self.assign(x.clone())?;
                assigns.push(x);
            } else {
                self.assign(x)?;
            }
        }
        Ok(assigns)
    }

    /// Expands an assignment: an array's elements as command words, and an
    /// index as an arithmetic expression.
    pub fn expand_assign(&mut self, a: &Assign) -> Result<Assignment, Flow> {
        let index = match &a.index {
            Some(w) => Some(self.arith_word(w)?),
            None => None,
        };
        let value = match a.array() {
            Some(items) => Value::Array(Box::new(self.expand_words(items)?)),
            None => Value::Str(self.expand_word_str(&a.value)?),
        };
        Ok(Assignment {
            name: a.name.clone(),
            index,
            append: a.append,
            value,
        })
    }

    /// Makes an assignment.
    pub fn assign(&mut self, a: Assignment) -> Result<(), Flow> {
        match (a.index, a.value) {
            (Some(i), Value::Str(v)) => self.set_element(&a.name, i, v, a.append),
            (Some(i), Value::Array(_)) => {
                self.error(format!(
                    "{}[{i}]: can't assign an array to an element",
                    String::from_utf8_lossy(&a.name)
                ));
                Err(Flow::Error(2))
            }
            (None, Value::Str(v)) if a.append => {
                let mut old = self.get_var(&a.name).unwrap_or_default();
                old.extend(v);
                self.set_var(&a.name, old)
            }
            (None, Value::Array(items)) if a.append => self.append_elements(&a.name, *items),
            (None, v) => self.set_var_value(&a.name, v),
        }
    }

    fn trace(&mut self, fd: i32, assigns: &[Assignment], argv: &[Vec<u8>]) {
        // As in dash (`inps4`), commands run while `PS4` is expanded (in a
        // command substitution in it) aren't traced, so there is no loop.
        if self.opt(Opt::Xtrace) && !self.in_ps4 && (!argv.is_empty() || !assigns.is_empty()) && fd >= 0 {
            self.in_ps4 = true;
            self.xtrace(fd, assigns, argv);
            self.in_ps4 = false;
        }
    }

    fn run_simple_redirected(
        &mut self,
        cmd: &SimpleCommand,
        mut argv: Vec<Vec<u8>>,
        no_fork: bool,
        err_fd: i32,
    ) -> ExecResult {
        if argv.is_empty() {
            let assigns = self.expand_assigns(cmd, true)?;
            self.trace(err_fd, &assigns, &argv);
            return Ok(self.subst_status.unwrap_or(0));
        }
        let mut kind = self.lookup_command(&argv[0], true);
        // `setopt cd.auto`, as in zsh: a command that is a single word, with
        // no redirections, read from standard input (so not in scripts or
        // `-c`), that names a directory and not a command, runs `cd`.
        if matches!(kind, CommandKind::External)
            && argv.len() == 1
            && cmd.redirs.is_empty()
            && self.opt(Opt::Autocd)
            && self.opt(Opt::Stdin)
            && let Some(dir) = self.autocd_target(&argv[0])
        {
            argv = vec![b"cd".to_vec(), b"--".to_vec(), dir];
            kind = CommandKind::Builtin(builtins::cd::cd);
        }
        let special = matches!(kind, CommandKind::Special(_));
        let assigns = self.expand_assigns(cmd, special)?;
        self.trace(err_fd, &assigns, &argv);
        if special && argv[0] == b"exec" {
            // As in dash, `exec` exports its assignments to the command.
            for a in &cmd.assigns {
                self.vars.entry(&a.name).exported = true;
            }
        }
        match kind {
            CommandKind::Special(f) => self.call_builtin(f, &argv),
            CommandKind::Function(body) => self.with_temp_assigns(assigns, |sh| sh.call_function(&body, &argv)),
            CommandKind::Builtin(f) => self.with_temp_assigns(assigns, |sh| sh.call_builtin(f, &argv)),
            // As in dash, the assignments are made (temporarily) in the shell,
            // so that an error in one is the shell's.
            CommandKind::External => self.with_temp_assigns(assigns, |sh| sh.run_external(cmd, &argv, no_fork)),
        }
    }

    /// The directory that `name`, a command that wasn't found, changes to
    /// under `setopt cd.auto` (zsh's `cancd`): a command in `PATH` or an
    /// executable file comes first, and a relative name not starting with
    /// `.` or `..` is also looked for in `CDPATH`.
    fn autocd_target(&mut self, name: &[u8]) -> Option<Vec<u8>> {
        let can = |p: &[u8]| sys::is_dir(p) && sys::access(p, libc::X_OK);
        if name.is_empty() || (!name.contains(&b'/') && name != b".." && self.find_in_path(name).is_some()) {
            return None;
        }
        let dotted = name == b"." || name == b".." || name.starts_with(b"./") || name.starts_with(b"../");
        if name[0] == b'/' || dotted {
            return can(name).then(|| name.to_vec());
        }
        // A directory here comes before `CDPATH`, unlike in `cd`.
        if can(name) {
            return Some([b"./", name].concat());
        }
        if sys::access(name, libc::X_OK) {
            return None;
        }
        let cdpath = self.get_var(b"CDPATH")?;
        cdpath.split(|&c| c == b':').find_map(|p| {
            let cand = if p.is_empty() {
                name.to_vec()
            } else {
                [p, if p.ends_with(b"/") { b"" } else { b"/" }, name].concat()
            };
            can(&cand).then_some(cand)
        })
    }

    fn run_external(&mut self, cmd: &SimpleCommand, argv: &[Vec<u8>], no_fork: bool) -> ExecResult {
        // Replace the shell process only if no trap needs it.
        let exec_now = no_fork && !self.has_traps();
        if !exec_now && self.can_spawn() {
            // Like dash's `vforkexec`.
            return Ok(match self.spawn_argv(argv, None) {
                Ok(pid) => self.wait_foreground(&[pid], || vec![cmdtext::simple(cmd)]),
                Err(status) => status,
            });
        }
        let pid = if exec_now {
            0
        } else {
            if !self.look_up_before_fork(argv) {
                return Ok(127);
            }
            self.fork_child(ForkKind::Foreground(0))?
        };
        if pid == 0 {
            self.exec_argv(argv, None);
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

    /// Looks up an external command in the shell before forking for it, so
    /// that the shell remembers it (`hash`; the child finds it in its copy of
    /// the cache) and a missing command costs no fork. Returns false, after
    /// reporting it, if there is no such command.
    fn look_up_before_fork(&mut self, argv: &[Vec<u8>]) -> bool {
        let name = &argv[0];
        if name.contains(&b'/') || self.find_in_path(name).is_some() {
            return true;
        }
        self.report_not_found(argv);
        false
    }

    /// Looks up the command of a pipeline's simple command before its process
    /// is forked, so that the shell remembers it (`hash`), as dash does. Only
    /// a literal name is looked up: expanding it could have side effects.
    pub fn remember_command(&mut self, cmd: &Command) {
        let Command::Simple(sc) = cmd else { return };
        let Some(Word(parts)) = sc.words.first() else { return };
        let [WordPart::Literal(name)] = &parts[..] else { return };
        if sc.assigns.iter().any(|a| a.name == b"PATH") || name.iter().any(|c| b"/*?[".contains(c)) {
            return;
        }
        if matches!(self.lookup_command(name, true), CommandKind::External) {
            self.find_in_path(name);
        }
    }

    /// Starts an external command without forking the shell (see
    /// `can_spawn`). Returns its pid, or the status if it couldn't be run.
    fn spawn_argv(&mut self, argv: &[Vec<u8>], alt_path: Option<&[u8]>) -> Result<i32, i32> {
        // A new job: dash frees a finished job's slot at this point.
        self.jobs.reclaim(false);
        let env = self.vars.environ();
        let r = self.with_command_path(&argv[0], alt_path, |sh, path| {
            let r = sys::spawn(path, argv, &env);
            if r == Err(libc::ENOEXEC) {
                // A script without `#!`: run it with this shell.
                return sys::spawn(&sh.self_exe(), &sh.script_args(path, argv), &env);
            }
            r
        });
        r.map_err(|e| self.exec_error(argv, e))
    }

    /// Runs `f` with variable assignments that only last for its duration.
    fn with_temp_assigns(&mut self, assigns: Vec<Assignment>, f: impl FnOnce(&mut Shell) -> ExecResult) -> ExecResult {
        let mut saved = Vec::with_capacity(assigns.len());
        let mut r = None;
        for a in assigns {
            let n = a.name.clone();
            let old = self.vars.take(&n);
            let set = self.assign(a);
            if set.is_ok() {
                self.vars.entry(&n).exported = true;
            }
            saved.push((n, old));
            if let Err(e) = set {
                r = Some(Err(e));
                break;
            }
        }
        let r = r.unwrap_or_else(|| f(self));
        for (n, old) in saved.into_iter().rev() {
            self.restore_var(n, old);
        }
        r
    }

    /// Runs a command given as expanded words, without assignments or
    /// redirections (for `command`; `alt_path` for `command -p`).
    pub fn run_argv(&mut self, argv: &[Vec<u8>], functions: bool, alt_path: Option<&[u8]>) -> ExecResult {
        match self.lookup_command(&argv[0], functions) {
            CommandKind::Special(f) | CommandKind::Builtin(f) => self.call_builtin(f, argv),
            CommandKind::Function(body) => self.call_function(&body, argv),
            CommandKind::External => {
                let pid = if self.can_spawn() {
                    match self.spawn_argv(argv, alt_path) {
                        Ok(pid) => pid,
                        Err(status) => return Ok(status),
                    }
                } else {
                    if alt_path.is_none() && !self.look_up_before_fork(argv) {
                        return Ok(127);
                    }
                    let pid = self.fork_child(ForkKind::Foreground(0))?;
                    if pid == 0 {
                        self.exec_argv(argv, alt_path);
                    }
                    pid
                };
                let text = || vec![String::from_utf8_lossy(&argv.join(&b' ')).into_owned()];
                Ok(self.wait_foreground(&[pid], text))
            }
        }
    }

    /// Replaces the process with an external command. Never returns.
    pub fn exec_argv(&mut self, argv: &[Vec<u8>], alt_path: Option<&[u8]>) -> ! {
        let env = self.vars.environ();
        let r: Result<(), i32> = self.with_command_path(&argv[0], alt_path, |sh, path| {
            let mut e = sys::execve(path, argv, &env);
            if e == libc::ENOEXEC {
                // A script without `#!`: run it with this shell.
                e = sys::execve(&sh.self_exe(), &sh.script_args(path, argv), &env);
            }
            Err(e)
        });
        let code = self.exec_error(argv, r.unwrap_err());
        sys::exit(code)
    }

    /// The arguments for running `path`, a script without `#!`, with this
    /// shell.
    fn script_args(&self, path: &[u8], argv: &[Vec<u8>]) -> Vec<Vec<u8>> {
        let mut args = vec![self.arg0.clone(), path.to_vec()];
        args.extend_from_slice(&argv[1..]);
        args
    }

    /// Reports that the command `argv` couldn't be executed (errno `e`),
    /// and returns the status for it.
    fn exec_error(&self, argv: &[Vec<u8>], e: i32) -> i32 {
        let name = &argv[0];
        // Debian's dash: 126 only for a file that can't be executed.
        let (msg, code) = match e {
            libc::ENOENT if !name.contains(&b'/') => {
                self.report_not_found(argv);
                return 127;
            }
            libc::ENOENT | libc::ENOTDIR => ("not found".to_string(), 127),
            libc::EACCES | libc::EISDIR => ("Permission denied".to_string(), 126),
            _ => (sys::strerror(e), 127),
        };
        self.error(format!("{}: {msg}", String::from_utf8_lossy(name)));
        code
    }

    /// Expands a prompt variable (`PS1`, `PS2`, `PS4`): the text to write.
    pub fn expand_prompt(&mut self, var: &[u8]) -> Vec<u8> {
        self.prompt(var).text
    }

    /// Expands a prompt variable: parameter expansion, then, with the
    /// `prompt.percent` option, `%` sequences (as zsh does).
    pub fn prompt(&mut self, var: &[u8]) -> crate::prompt::Prompt {
        let text = self.param_expand_prompt(var);
        self.percent_expand_prompt(text)
    }

    /// Expands the `%` sequences of a prompt if `prompt.percent` is on.
    pub fn percent_expand_prompt(&self, text: Vec<u8>) -> crate::prompt::Prompt {
        if self.opt(Opt::PromptPercent) && text.contains(&b'%') {
            crate::prompt::expand(self, &text)
        } else {
            crate::prompt::Prompt::plain(text)
        }
    }

    /// Parameter-expands a prompt variable.
    pub fn param_expand_prompt(&mut self, var: &[u8]) -> Vec<u8> {
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
