//! Interpreter state and the top-level read-parse-execute loop.

use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::FunctionBody;
use crate::input::{Input, Line};
use crate::jobs::JobTable;
use crate::lexer::{AliasMap, ParseError, Parser};
use crate::options::{Opt, Options};
use crate::signals::{self, NSIG};
use crate::sys;
use crate::vars::{Var, Vars};

/// Non-local control flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Break(usize),
    Continue(usize),
    Return(i32),
    Exit(i32),
    /// A shell error (syntax error, expansion error, failed special
    /// built-in): a non-interactive shell exits, an interactive one returns
    /// to the prompt.
    Error(i32),
}

pub type ExecResult = Result<i32, Flow>;

pub struct Shell {
    pub vars: Vars,
    pub positional: Vec<Vec<u8>>,
    pub arg0: Vec<u8>,
    pub last_status: i32,
    pub last_bg_pid: Option<i32>,
    pub options: Options,
    pub functions: HashMap<Vec<u8>, Rc<FunctionBody>>,
    pub aliases: Rc<AliasMap>,
    /// Trap actions by signal number (0 is EXIT). An empty action ignores
    /// the signal.
    pub traps: Vec<Option<Vec<u8>>>,
    pub ignored_on_entry: [bool; NSIG],
    pub jobs: JobTable,
    pub hash: HashMap<Vec<u8>, Vec<u8>>,
    pub interactive: bool,
    pub in_subshell: bool,
    pub loop_depth: usize,
    pub func_depth: usize,
    pub dot_depth: usize,
    pub errexit_suppressed: usize,
    pub lineno: u32,
    /// `$$`: the pid of the main shell process, also in subshells.
    pub pid: i32,
    /// Exit status of the last command substitution in the current command.
    pub subst_status: Option<i32>,
    /// Saved variables for `local`, one frame per function call.
    pub locals: Vec<Vec<(Vec<u8>, Option<Var>)>>,
    /// Path of this executable, used to run scripts without `#!`.
    pub self_exe: Vec<u8>,
    /// Position inside a group of options for `getopts`.
    pub getopts_offset: usize,
    pub getopts_optind: Vec<u8>,
    /// Currently running the EXIT trap.
    pub in_exit_trap: bool,
}

impl Shell {
    pub fn new() -> Shell {
        let mut vars = Vars::from_env();
        let pid = std::process::id() as i32;
        let _ = vars.set(b"IFS", b" \t\n".to_vec());
        let _ = vars.set(b"OPTIND", b"1".to_vec());
        let ps1: &[u8] = if sys::geteuid() == 0 { b"# " } else { b"$ " };
        for (k, v) in [(&b"PS1"[..], ps1), (b"PS2", b"> "), (b"PS4", b"+ ")] {
            if vars.get(k).is_none() {
                let _ = vars.set(k, v.to_vec());
            }
        }
        let _ = vars.set(b"PPID", sys::getppid().to_string().into_bytes());
        if vars.get(b"PATH").is_none() {
            let _ = vars.set(
                b"PATH",
                b"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_vec(),
            );
        }
        // PWD is inherited only if it names the current directory.
        let cwd = sys::getcwd();
        let pwd_ok = match (vars.get(b"PWD"), &cwd) {
            (Some(p), Some(c)) => p.first() == Some(&b'/') && sys::same_file(p, c),
            _ => false,
        };
        if !pwd_ok && let Some(c) = cwd {
            let _ = vars.set(b"PWD", c);
        }
        vars.entry(b"PWD").exported = true;
        let self_exe = std::env::current_exe()
            .map(|p| {
                use std::os::unix::ffi::OsStrExt;
                p.as_os_str().as_bytes().to_vec()
            })
            .unwrap_or_else(|_| b"/proc/self/exe".to_vec());
        Shell {
            vars,
            positional: Vec::new(),
            arg0: b"luish".to_vec(),
            last_status: 0,
            last_bg_pid: None,
            options: Options::default(),
            functions: HashMap::new(),
            aliases: Rc::new(AliasMap::new()),
            traps: vec![None; NSIG],
            ignored_on_entry: signals::ignored_on_entry(),
            jobs: JobTable::default(),
            hash: HashMap::new(),
            interactive: false,
            in_subshell: false,
            loop_depth: 0,
            func_depth: 0,
            dot_depth: 0,
            errexit_suppressed: 0,
            lineno: 0,
            pid,
            subst_status: None,
            locals: Vec::new(),
            self_exe,
            getopts_offset: 0,
            getopts_optind: b"1".to_vec(),
            in_exit_trap: false,
        }
    }

    pub fn opt(&self, o: Opt) -> bool {
        self.options.get(o)
    }

    // ------------------------------------------------------------------
    // Variables

    pub fn get_var(&self, name: &[u8]) -> Option<Vec<u8>> {
        if name == b"LINENO" {
            return Some(self.lineno.to_string().into_bytes());
        }
        self.vars.get(name).map(|v| v.to_vec())
    }

    /// Sets a variable, reporting an error if it is readonly.
    pub fn set_var(&mut self, name: &[u8], value: Vec<u8>) -> Result<(), Flow> {
        if self.vars.set(name, value).is_err() {
            self.error(format!("{}: is read only", String::from_utf8_lossy(name)));
            return Err(Flow::Error(2));
        }
        if self.opt(Opt::Allexport) {
            self.vars.entry(name).exported = true;
        }
        self.var_changed(name);
        Ok(())
    }

    pub fn var_changed(&mut self, name: &[u8]) {
        if name == b"PATH" {
            self.hash.clear();
        } else if name == b"OPTIND" {
            self.getopts_offset = 0;
        }
    }

    // ------------------------------------------------------------------
    // Errors

    /// Prints `$0: LINENO: msg` to stderr.
    pub fn error(&self, msg: impl AsRef<[u8]>) {
        let mut s = self.arg0.clone();
        s.extend_from_slice(b": ");
        if !self.interactive || self.lineno > 0 {
            s.extend_from_slice(self.lineno.to_string().as_bytes());
            s.extend_from_slice(b": ");
        }
        s.extend_from_slice(msg.as_ref());
        s.push(b'\n');
        sys::write_all(2, &s);
    }

    pub fn syntax_error(&mut self, e: &ParseError) {
        self.lineno = e.lineno;
        self.error(&e.msg);
    }

    // ------------------------------------------------------------------
    // Running code

    /// Parses and runs a string in the current shell (`eval`, `.`, traps).
    pub fn run_string(&mut self, text: &[u8]) -> ExecResult {
        let lineno = self.lineno;
        let mut p = Parser::new(text.to_vec(), lineno.max(1), true);
        let mut status = 0;
        loop {
            let aliases = self.aliases.clone();
            match p.parse_next(&aliases) {
                Ok(Some(list)) => {
                    if !self.opt(Opt::Noexec) || self.interactive {
                        status = self.run_list(&list)?;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    self.syntax_error(&e);
                    return Err(Flow::Error(2));
                }
            }
        }
        Ok(status)
    }

    /// Runs any traps for signals that have arrived.
    pub fn run_pending_traps(&mut self) -> Result<(), Flow> {
        if !signals::any_pending() {
            return Ok(());
        }
        for sig in signals::take_pending() {
            match self.traps[sig].clone() {
                Some(action) if !action.is_empty() => {
                    let saved = self.last_status;
                    let saved_lineno = self.lineno;
                    match self.run_string(&action) {
                        Err(Flow::Exit(n)) => return Err(Flow::Exit(n)),
                        Err(Flow::Error(n)) if !self.interactive => return Err(Flow::Exit(n)),
                        _ => {}
                    }
                    self.last_status = saved;
                    self.lineno = saved_lineno;
                }
                Some(_) => {}
                None if sig == libc::SIGINT as usize && self.interactive => {
                    sys::write_all(2, b"\n");
                    return Err(Flow::Error(128 + libc::SIGINT));
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Exits the shell (or subshell): runs the EXIT trap and terminates the
    /// process.
    pub fn exit(&mut self, status: i32) -> ! {
        let mut status = status;
        if !self.in_exit_trap
            && let Some(action) = self.traps[0].take()
        {
            self.in_exit_trap = true;
            self.last_status = status;
            match self.run_string(&action) {
                Err(Flow::Exit(n)) | Err(Flow::Error(n)) => status = n,
                _ => {}
            }
        }
        if !self.in_subshell {
            crate::interactive::save_history(self);
        }
        sys::exit(status)
    }

    /// Runs the main input loop until end of input.
    pub fn run_input(&mut self, input: &mut Input) -> i32 {
        if let Some(text) = input.whole_text() {
            self.run_whole(text)
        } else {
            self.run_incremental(input)
        }
    }

    /// Handles the result of running one top-level command.
    fn top_level_result(&mut self, r: ExecResult) {
        match r {
            Ok(s) => self.last_status = s,
            Err(Flow::Exit(n)) => self.exit(n),
            Err(Flow::Error(n)) => {
                if !self.interactive {
                    self.exit(n);
                }
                self.last_status = n;
            }
            // `break`/`continue` outside a loop, `return` outside a function
            Err(Flow::Break(_)) | Err(Flow::Continue(_)) => {}
            Err(Flow::Return(n)) => {
                if !self.interactive {
                    self.exit(n);
                }
                self.last_status = n;
            }
        }
    }

    fn run_whole(&mut self, text: Vec<u8>) -> i32 {
        let mut p = Parser::new(text, 1, true);
        loop {
            let start = p.consumed();
            let aliases = self.aliases.clone();
            match p.parse_next(&aliases) {
                Ok(Some(list)) => {
                    if self.opt(Opt::Verbose) {
                        let end = p.consumed().min(p.src.len());
                        sys::write_all(2, &p.src[start.min(end)..end]);
                    }
                    if !self.opt(Opt::Noexec) {
                        let r = self.run_list(&list);
                        self.top_level_result(r);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    self.syntax_error(&e);
                    self.exit(2);
                }
            }
        }
        self.last_status
    }

    fn run_incremental(&mut self, input: &mut Input) -> i32 {
        let mut buf: Vec<u8> = Vec::new();
        let mut lineno = 1u32;
        let mut eof = false;
        let mut continuation = false;
        loop {
            let mut p = Parser::new(buf.clone(), lineno, eof);
            let aliases = self.aliases.clone();
            match p.parse_next(&aliases) {
                Ok(Some(list)) => {
                    let used = p.consumed().min(buf.len());
                    let text: Vec<u8> = buf.drain(..used).collect();
                    lineno = p.lineno;
                    continuation = false;
                    if self.opt(Opt::Verbose) && !self.interactive {
                        sys::write_all(2, &text);
                    }
                    input.add_history(&text);
                    if !self.opt(Opt::Noexec) || self.interactive {
                        let r = self.run_list(&list);
                        self.top_level_result(r);
                    }
                }
                Ok(None) => break,
                Err(e) if e.incomplete && !eof => {
                    if !p.started {
                        // only blank lines and comments so far
                        lineno += buf.iter().filter(|&&c| c == b'\n').count() as u32;
                        buf.clear();
                        continuation = false;
                    }
                    match input.read_line(self, continuation) {
                        Line::Text(line) => {
                            if self.opt(Opt::Verbose) && self.interactive {
                                sys::write_all(2, &line);
                            }
                            buf.extend_from_slice(&line);
                            continuation = true;
                        }
                        Line::Interrupted => {
                            lineno += buf.iter().filter(|&&c| c == b'\n').count() as u32;
                            buf.clear();
                            continuation = false;
                            self.last_status = 128 + libc::SIGINT;
                        }
                        Line::Eof => {
                            eof = true;
                            if self.interactive && buf.is_empty() && self.opt(Opt::Ignoreeof) {
                                sys::write_all(2, b"Use \"exit\" to leave shell.\n");
                                eof = false;
                            }
                        }
                    }
                }
                Err(e) => {
                    self.syntax_error(&e);
                    if !self.interactive {
                        self.exit(2);
                    }
                    self.last_status = 2;
                    lineno += buf.iter().filter(|&&c| c == b'\n').count() as u32;
                    buf.clear();
                    continuation = false;
                }
            }
        }
        self.last_status
    }
}
