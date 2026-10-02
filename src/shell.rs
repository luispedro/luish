//! Interpreter state and the top-level read-parse-execute loop.

use crate::frames::{Frame, SourceFile};
use crate::hash::HashMap;
use std::rc::Rc;

use crate::ast::FunctionBody;
use crate::input::{Input, Line};
use crate::interactive::Expanded;
use crate::jobs::{JobTable, Terminal};
use crate::lexer::{AliasMap, ParseError, Parser};
use crate::options::{Opt, Options};
use crate::signals::{self, NSIG};
use crate::sys;
use crate::vars::{AssignError, Item, Saved, Special, Subscript, Transform, Value, Var, Vars};

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

/// A shell function.
#[derive(Clone)]
pub struct Function {
    pub name: Rc<[u8]>,
    pub body: Rc<FunctionBody>,
    /// The file it was defined in (`BASH_SOURCE` while it runs).
    pub file: Option<Rc<SourceFile>>,
    /// Whether the line numbers of its body are lines of `file` (see
    /// [`Frame::lines_in_file`]).
    pub lines_in_file: bool,
    /// The lines of its body in `file`, for a function read back from a
    /// saved state, until its first call gives them to the body
    /// (`__luish_internal function-file`, [`Shell::call_function`]): most
    /// functions of the startup cache are never called.
    pub pending_lines: Option<Rc<[u8]>>,
}

pub struct Shell {
    pub vars: Vars,
    pub positional: Vec<Vec<u8>>,
    pub arg0: Vec<u8>,
    pub last_status: i32,
    /// The statuses of the commands of the last pipeline, for `pipestatus`.
    pub pipestatus: Vec<i32>,
    pub last_bg_pid: Option<i32>,
    pub options: Options,
    pub functions: HashMap<Vec<u8>, Function>,
    /// The call stack, innermost last (`frames.rs`).
    pub frames: Vec<Frame>,
    /// How many strings (`eval`, traps, ...) run in the innermost frame:
    /// their line numbers aren't lines of its file.
    pub in_string: u32,
    /// The index in the shell's arguments of the `-c` command, whose lines
    /// error messages show.
    pub command_arg: Option<usize>,
    pub aliases: Rc<AliasMap>,
    /// Trap actions by signal number (0 is EXIT). An empty action ignores
    /// the signal.
    pub traps: Vec<Option<Vec<u8>>>,
    pub jobs: JobTable,
    /// The terminal, while job control is on (only in the main shell).
    pub jobctl: Option<Terminal>,
    /// 2 just after warning about stopped jobs, 1 for the command after
    /// that: a second `exit` in a row exits anyway.
    pub job_warning: u8,
    /// Commands found in `PATH`: the file and the index of its directory.
    pub hash: HashMap<Vec<u8>, (Vec<u8>, usize)>,
    /// The `PATH` directories when last checked (interactive only).
    pub path_stamps: Vec<crate::path::DirStamp>,
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
    /// The `<(...)` and `>(...)` substitutions of the commands running now,
    /// closed when their command ends (`exec/procsubst.rs`).
    pub procsubs: Vec<crate::exec::ProcSub>,
    /// Processes of substitutions that may outlive their command, to reap.
    pub procsub_orphans: Vec<i32>,
    /// Saved variables for `local`, one frame per function call.
    pub locals: Vec<Vec<(Vec<u8>, Saved)>>,
    /// Position inside a group of options for `getopts`.
    /// Writing a built-in's output failed (checked after it returns).
    pub out_failed: std::cell::Cell<bool>,
    /// `PS4` is being expanded for `set -x` (dash's `inps4`).
    pub in_ps4: bool,
    /// The logical current directory (dash's `curdir`), kept by `cd` and
    /// printed by `pwd`; `None` if it couldn't be found.
    pub curdir: Option<Vec<u8>>,
    /// The directory stack (`pushd`, `popd`, `dirs`), most recent first,
    /// without the current directory.
    pub dirstack: Vec<Vec<u8>>,
    /// The line editor's key bindings that `bindkey` changed.
    pub keymap: crate::interactive::keys::Keymap,
    /// The styles of the line editor (`style`, `config.toml`).
    pub styles: crate::style::Styles,
    /// `getopts`'s position (dash's `shellparam.optind` and `optoff`).
    pub optind: usize,
    pub optoff: Option<usize>,
    /// Currently running the EXIT trap.
    pub in_exit_trap: bool,
    /// While an entry of a startup cache is built: the absolute paths of
    /// the files read by `.`.
    pub sourced_files: Option<Vec<Vec<u8>>>,
    /// The plugin host, created by the first `plugin load`.
    pub plugins: Option<Rc<crate::plugins::Host>>,
    /// `--no-plugins`: `plugin load` does nothing.
    pub no_plugins: bool,
    /// Set while the startup files of `rc.d` run, with the plugins that
    /// `config.toml` enables: plugins then wait for the end of `rc.d` to run
    /// their `post-rc.lsh` and `post-rc` hooks (`plugins/mod.rs`).
    pub in_rc: bool,
    /// `--internal-check-cache=NAME:PATH`, in the shell that
    /// `__luish_internal check-cache` starts: rebuild the startup cache
    /// `NAME` into `PATH` and exit (`startcache::check_child`).
    pub check_cache: Option<(Vec<u8>, Vec<u8>)>,
    /// While a startup file with `__luish_cache` blocks runs: the cache
    /// they use (`startcache::run_block`).
    pub startcache: Option<Box<crate::startcache::Run>>,
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
        // PWD is inherited only if it names the current directory (then,
        // as in dash, there is no need for getcwd).
        let curdir = vars
            .get(b"PWD")
            .filter(|p| p.first() == Some(&b'/') && sys::same_file(p, b"."))
            .map(|p| p.to_vec())
            .or_else(sys::getcwd);
        if let Some(c) = &curdir {
            let _ = vars.set(b"PWD", c.clone());
        }
        vars.entry(b"PWD").exported = true;
        Shell {
            vars,
            positional: Vec::new(),
            arg0: b"luish".to_vec(),
            last_status: 0,
            pipestatus: Vec::new(),
            last_bg_pid: None,
            options: Options::default(),
            functions: HashMap::default(),
            frames: Vec::new(),
            in_string: 0,
            command_arg: None,
            aliases: Rc::new(AliasMap::default()),
            traps: vec![None; NSIG],
            jobs: JobTable::default(),
            jobctl: None,
            job_warning: 0,
            hash: HashMap::default(),
            path_stamps: Vec::new(),
            interactive: false,
            in_subshell: false,
            loop_depth: 0,
            func_depth: 0,
            dot_depth: 0,
            errexit_suppressed: 0,
            lineno: 0,
            pid,
            subst_status: None,
            procsubs: Vec::new(),
            procsub_orphans: Vec::new(),
            locals: Vec::new(),
            optind: 1,
            optoff: None,
            curdir,
            dirstack: Vec::new(),
            keymap: Default::default(),
            styles: Default::default(),
            in_ps4: false,
            out_failed: std::cell::Cell::new(false),
            in_exit_trap: false,
            sourced_files: None,
            plugins: None,
            no_plugins: false,
            in_rc: false,
            check_cache: None,
            startcache: None,
        }
    }

    /// Path of this executable, used to run scripts without `#!`.
    pub fn self_exe(&self) -> Vec<u8> {
        std::env::current_exe()
            .map(|p| {
                use std::os::unix::ffi::OsStrExt;
                p.as_os_str().as_bytes().to_vec()
            })
            .unwrap_or_else(|_| b"/proc/self/exe".to_vec())
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
        if let Some(v) = self.vars.get(name) {
            return Some(v.to_vec());
        }
        self.special_value(name)
    }

    /// Increments `SHLVL`, as zsh does. It is set to 1 where it wasn't set
    /// only in an interactive shell, so that scripts see the same
    /// environment as under dash.
    pub fn bump_shlvl(&mut self, interactive: bool) {
        let level = match self.vars.get(b"SHLVL") {
            Some(v) => crate::expand::arith::parse_number(v).unwrap_or(0) + 1,
            None if interactive => 1,
            None => return,
        };
        if self.vars.set(b"SHLVL", level.to_string().into_bytes()).is_ok() {
            self.vars.entry(b"SHLVL").exported = true;
        }
    }

    /// The value of a special parameter such as `RANDOM` (`vars.rs`), if
    /// `name` is one that is still special.
    pub fn special_value(&self, name: &[u8]) -> Option<Vec<u8>> {
        match self.vars.special(name)? {
            Special::Histcmd => Some(crate::interactive::histcmd().to_string().into_bytes()),
            // As for any array, its first element.
            Special::Pipestatus | Special::PipestatusBash => Some(self.pipestatus.first()?.to_string().into_bytes()),
            Special::Path => Some(self.vars.get(b"PATH")?.split(|&c| c == b':').next()?.to_vec()),
            Special::Dirstack => self.dirstack.first().cloned(),
            s @ (Special::BashSource | Special::Funcname | Special::BashLineno) => {
                self.stack_elements(s)?.into_iter().next()
            }
            s => Some(self.vars.special_value(s)),
        }
    }

    /// The elements of a variable that isn't stored, for `${a[@]}` and
    /// `${a[i]}`: `pipestatus`, `path`, `dirstack`, the call stack's arrays,
    /// or one element for `LINENO` or another special.
    pub fn special_elements(&self, name: &[u8]) -> Option<Vec<Vec<u8>>> {
        match self.vars.special(name) {
            Some(Special::Pipestatus | Special::PipestatusBash) => {
                Some(self.pipestatus.iter().map(|s| s.to_string().into_bytes()).collect())
            }
            Some(s @ (Special::Path | Special::Dirstack)) => Some(self.tied_elements(s)),
            Some(s @ (Special::BashSource | Special::Funcname | Special::BashLineno)) => self.stack_elements(s),
            _ => self.get_var(name).map(|v| vec![v]),
        }
    }

    /// A special parameter that is still special, as a variable, for
    /// `typeset -p`: its value (an array for `pipestatus`, `path` and
    /// `dirstack`), with the attributes that `export` or `readonly` gave it.
    pub fn special_var(&self, name: &[u8]) -> Option<Var> {
        let value = match self.vars.special(name)?.is_array() {
            true => Value::Array(Box::new(self.special_elements(name)?)),
            false => Value::Str(self.special_value(name)?),
        };
        let attrs = self.vars.var(name);
        Some(Var {
            value: Some(value),
            exported: attrs.is_some_and(|v| v.exported),
            readonly: attrs.is_some_and(|v| v.readonly),
            transform: attrs.map(|v| v.transform).unwrap_or_default(),
            assigned: false,
        })
    }

    /// The elements of a tied array: the directories of `PATH` for `path`
    /// (as in zsh, an empty `PATH` is one empty directory, and an unset one
    /// none), or the directory stack for `dirstack`.
    fn tied_elements(&self, s: Special) -> Vec<Vec<u8>> {
        match s {
            Special::Dirstack => self.dirstack.clone(),
            _ => self
                .vars
                .get(b"PATH")
                .map_or_else(Vec::new, |p| p.split(|&c| c == b':').map(<[u8]>::to_vec).collect()),
        }
    }

    /// The special `name` names if it is an array tied to something else
    /// (`path` or `dirstack`) and still special.
    fn tied(&self, name: &[u8]) -> Option<Special> {
        self.vars.special(name).filter(|s| s.is_tied())
    }

    /// An array assignment to a tied array: sets `PATH`, or replaces the
    /// directory stack (whose directories aren't checked, as in zsh).
    fn assign_tied(&mut self, s: Special, elements: Vec<Vec<u8>>) -> Result<(), String> {
        let name = s.name();
        if self.vars.var(name).is_some_and(|v| v.readonly) {
            return Err(readonly_message(name));
        }
        match s {
            Special::Dirstack => {
                self.dirstack = elements;
                Ok(())
            }
            _ => self.try_set_var(b"PATH", elements.join(&b':')),
        }
    }

    /// Sets a variable, reporting an error if it is readonly.
    pub fn set_var(&mut self, name: &[u8], value: Vec<u8>) -> Result<(), Flow> {
        self.try_set_var(name, value).map_err(|msg| {
            self.error(msg);
            Flow::Error(2)
        })
    }

    /// Sets a variable, or returns the error message.
    pub fn try_set_var(&mut self, name: &[u8], value: Vec<u8>) -> Result<(), String> {
        // As in dash (`getoptsreset`), OPTIND must be a number.
        if name == b"OPTIND" && crate::builtins::parse_uint(&value).is_none() {
            return Err(format!("Illegal number: {}", String::from_utf8_lossy(&value)));
        }
        let value = match self.vars.transform(name) {
            t if t.unique && name == b"PATH" => {
                // `typeset -U PATH` removes repeated directories, as in zsh.
                let mut dirs = self
                    .convert(t, value)?
                    .split(|&c| c == b':')
                    .map(<[u8]>::to_vec)
                    .collect();
                crate::vars::dedupe(&mut dirs);
                dirs.join(&b':')
            }
            t if t.any() => self.convert(t, value)?,
            _ => value,
        };
        if self.vars.set(name, value).is_err() {
            return Err(readonly_message(name));
        }
        if self.opt(Opt::Allexport) {
            self.vars.entry(name).exported = true;
        }
        self.var_changed(name);
        Ok(())
    }

    /// The value to store in a variable (or an element of one) with the
    /// attributes `t`: for an integer variable (`typeset -i`), `value`
    /// evaluated as an arithmetic expression, in decimal; with `-l` or `-u`,
    /// in lower or upper case.
    fn convert(&mut self, t: Transform, mut value: Vec<u8>) -> Result<Vec<u8>, String> {
        if t.integer {
            value = crate::expand::arith::eval(self, &value)?.to_string().into_bytes();
        }
        if t.lower {
            value.make_ascii_lowercase();
        } else if t.upper {
            value.make_ascii_uppercase();
        }
        Ok(value)
    }

    /// Converts elements to assign with the attributes `t` (see `convert`),
    /// but doesn't remove repeated ones (`-U`).
    fn convert_all<'a>(&mut self, t: Transform, elements: impl Iterator<Item = &'a mut Vec<u8>>) -> Result<(), String> {
        if t.any() {
            for e in elements {
                *e = self.convert(t, std::mem::take(e))?;
            }
        }
        Ok(())
    }

    /// Reports an error that exits a non-interactive shell.
    pub fn fail(&self, msg: impl AsRef<[u8]>) -> Flow {
        self.error(msg);
        Flow::Error(2)
    }

    /// Assigns a whole value (an array), reporting an error if the variable
    /// is readonly.
    pub fn set_var_value(&mut self, name: &[u8], value: Value) -> Result<(), Flow> {
        self.try_set_var_value(name, value).map_err(|msg| self.fail(msg))
    }

    /// Assigns a whole value, or returns the error message.
    pub fn try_set_var_value(&mut self, name: &[u8], value: Value) -> Result<(), String> {
        let t = self.vars.transform(name);
        let value = match value {
            Value::Str(s) => return self.try_set_var(name, s),
            Value::Array(mut a) if t.any() => {
                self.convert_all(t, a.iter_mut())?;
                if t.unique {
                    crate::vars::dedupe(&mut a);
                }
                Value::Array(a)
            }
            Value::Assoc(mut h) if t.any() => {
                self.convert_all(t, h.values_mut().iter_mut())?;
                Value::Assoc(h)
            }
            v => v,
        };
        if matches!(value, Value::Array(_))
            && let Some(s) = self.tied(name)
        {
            return self.assign_tied(s, value.elements().to_vec());
        }
        if self.vars.set_value(name, value).is_err() {
            return Err(readonly_message(name));
        }
        self.after_assign(name);
        Ok(())
    }

    /// Assigns to (or with `append`, appends to) an element of an array: an
    /// index counts from the end if it is negative.
    pub fn set_element(&mut self, name: &[u8], sub: &Subscript, value: Vec<u8>, append: bool) -> Result<(), Flow> {
        let t = self.vars.transform(name);
        let (value, append) = if t.any() {
            let old = match (self.vars.get_value(name), sub) {
                _ if !append => None,
                (Some(Value::Assoc(h)), Subscript::Key(k)) => h.get(k),
                (Some(v), &Subscript::Index(i)) => {
                    let e = v.elements();
                    e.get(if i < 0 { i + e.len() as i64 } else { i } as usize)
                }
                _ => None,
            };
            let value = match old {
                // `a[i]+=expr` adds.
                Some(o) if t.integer => {
                    let o = crate::expand::arith::parse_number(o).unwrap_or(0);
                    let n = crate::expand::arith::eval(self, &value).map_err(|msg| self.fail(msg))?;
                    n.wrapping_add(o).to_string().into_bytes()
                }
                Some(o) => {
                    let mut o = o.clone();
                    o.extend(value);
                    self.convert(t, o).map_err(|msg| self.fail(msg))?
                }
                None => self.convert(t, value).map_err(|msg| self.fail(msg))?,
            };
            (value, false)
        } else {
            (value, append)
        };
        let r = match sub {
            Subscript::Index(i) if let Some(s) = self.tied(name) => {
                let mut a = Some(Value::Array(Box::new(self.tied_elements(s))));
                match crate::vars::set_index(&mut a, *i, value, append) {
                    Ok(()) => {
                        let mut a = a.unwrap().elements().to_vec();
                        if t.unique {
                            crate::vars::dedupe(&mut a);
                        }
                        return self.assign_tied(s, a).map_err(|msg| self.fail(msg));
                    }
                    Err(e) => Err(e),
                }
            }
            _ => self.vars.set_element(name, sub, value, append),
        };
        match r {
            Ok(()) => {
                if t.unique
                    && let Some(Value::Array(a)) = self.vars.get_value_mut(name)
                {
                    crate::vars::dedupe(a);
                }
                self.after_assign(name);
                Ok(())
            }
            Err(AssignError::Readonly) => Err(self.readonly_error(name)),
            Err(AssignError::BadSubscript) => {
                self.error(format!("{}[{sub}]: bad array subscript", String::from_utf8_lossy(name)));
                Err(Flow::Error(2))
            }
        }
    }

    /// Assigns (or with `append`, adds) the elements of `name=(...)`. To an
    /// associative array, they are pairs of keys and values, or all
    /// `[key]=value` (as in zsh). Otherwise `[i]=` gives the index of an
    /// element, and those that follow come after it (as in zsh and bash).
    pub fn assign_items(&mut self, name: &[u8], items: Vec<Item>, append: bool) -> Result<(), Flow> {
        let bad = |sh: &Shell, msg: &str| {
            sh.error(format!("{}: {msg}", String::from_utf8_lossy(name)));
            Flow::Error(2)
        };
        if self.vars.is_assoc(name) {
            let keyed = items.iter().filter(|i| i.key.is_some()).count();
            let pairs: Vec<(Vec<u8>, Vec<u8>)> = if keyed == items.len() {
                items.into_iter().map(|i| (i.key.unwrap(), i.value)).collect()
            } else if keyed > 0 {
                return Err(bad(self, "bad [key]=value syntax for associative array"));
            } else if !items.len().is_multiple_of(2) {
                return Err(bad(self, "bad set of key/value pairs for associative array"));
            } else {
                let mut items = items.into_iter();
                std::iter::from_fn(|| Some((items.next()?.value, items.next()?.value))).collect()
            };
            let mut pairs = pairs;
            self.convert_all(self.vars.transform(name), pairs.iter_mut().map(|(_, v)| v))
                .map_err(|msg| self.fail(msg))?;
            if self.vars.set_pairs(name, pairs, append).is_err() {
                return Err(self.readonly_error(name));
            }
            self.after_assign(name);
            return Ok(());
        }
        if items.iter().all(|i| i.key.is_none()) {
            let values = items.into_iter().map(|i| i.value).collect();
            return if append {
                self.append_elements(name, values)
            } else {
                self.set_var_value(name, Value::Array(Box::new(values)))
            };
        }
        let mut a = match self.vars.get_value(name) {
            Some(v) if append => v.elements().to_vec(),
            _ => Vec::new(),
        };
        let mut pos = a.len();
        for item in items {
            if let Some(key) = item.key {
                let i = crate::expand::arith::eval(self, &key).map_err(|msg| bad(self, &msg))?;
                let i = if i < 0 { i + a.len() as i64 } else { i };
                if i < 0 {
                    return Err(bad(self, "bad array subscript"));
                }
                pos = i as usize;
            }
            if pos > crate::vars::MAX_INDEX as usize {
                return Err(bad(self, "bad array subscript"));
            }
            if pos >= a.len() {
                a.resize(pos + 1, Vec::new());
            }
            a[pos] = item.value;
            pos += 1;
        }
        self.set_var_value(name, Value::Array(Box::new(a)))
    }

    /// Appends elements to an array (`a+=(x y)`).
    pub fn append_elements(&mut self, name: &[u8], mut items: Vec<Vec<u8>>) -> Result<(), Flow> {
        let t = self.vars.transform(name);
        self.convert_all(t, items.iter_mut()).map_err(|msg| self.fail(msg))?;
        if let Some(s) = self.tied(name) {
            let mut a = self.tied_elements(s);
            a.extend(items);
            if t.unique {
                crate::vars::dedupe(&mut a);
            }
            return self.assign_tied(s, a).map_err(|msg| self.fail(msg));
        }
        if self.vars.append_elements(name, items).is_err() {
            return Err(self.readonly_error(name));
        }
        if t.unique
            && let Some(Value::Array(a)) = self.vars.get_value_mut(name)
        {
            crate::vars::dedupe(a);
        }
        self.after_assign(name);
        Ok(())
    }

    fn readonly_error(&self, name: &[u8]) -> Flow {
        self.fail(readonly_message(name))
    }

    fn after_assign(&mut self, name: &[u8]) {
        if self.opt(Opt::Allexport) {
            self.vars.entry(name).exported = true;
        }
        self.var_changed(name);
    }

    /// Restarts `getopts` at the first argument (new positional parameters).
    pub fn reset_getopts(&mut self) {
        self.optind = 1;
        self.optoff = None;
    }

    /// Puts back a variable as it was (`None` to unset it).
    pub fn restore_var(&mut self, name: Vec<u8>, var: Option<Var>) {
        if matches!(&name[..], b"PATH" | b"OPTIND") {
            self.vars.restore(name.clone(), var);
            self.var_changed(&name);
        } else {
            self.vars.restore(name, var);
        }
    }

    /// Puts back a variable saved by `local` or a temporary assignment.
    pub fn restore_saved(&mut self, name: Vec<u8>, saved: Saved) {
        if matches!(&name[..], b"PATH" | b"OPTIND") {
            self.vars.restore_saved(name.clone(), saved);
            self.var_changed(&name);
        } else {
            self.vars.restore_saved(name, saved);
        }
    }

    pub fn var_changed(&mut self, name: &[u8]) {
        if name == b"PATH" {
            self.hash.clear();
        } else if name == b"OPTIND" {
            // dash's `getoptsreset`.
            let v = self.vars.get(b"OPTIND").unwrap_or_default();
            self.optind = crate::builtins::parse_uint(v).filter(|&n| n > 0).unwrap_or(1) as usize;
            self.optoff = None;
        }
    }

    // ------------------------------------------------------------------
    // Errors

    /// Prints `FILE: LINENO: msg` to stderr, where `FILE` is the file of
    /// the code running (or `$0`), then the text of that line and the call
    /// stack (`frames.rs`).
    pub fn error(&self, msg: impl AsRef<[u8]>) {
        let (file, line) = self.error_location();
        let mut s = file.to_vec();
        s.extend_from_slice(b": ");
        if let Some(line) = line {
            s.extend_from_slice(line.to_string().as_bytes());
            s.extend_from_slice(b": ");
        }
        s.extend_from_slice(msg.as_ref());
        s.push(b'\n');
        if let Some(text) = self.error_line_text() {
            s.extend_from_slice(b"    ");
            s.extend(text);
            s.push(b'\n');
        }
        s.extend(self.stack_trace());
        sys::write_all(2, &s);
    }

    pub fn syntax_error(&mut self, e: &ParseError) {
        self.lineno = e.lineno;
        self.error(&e.msg);
    }

    // ------------------------------------------------------------------
    // Running code

    /// Parses and runs a string in the current shell (`eval`, traps). Its
    /// lines count on from the current line, so they aren't lines of the
    /// file (`in_string`).
    pub fn run_string(&mut self, text: &[u8]) -> ExecResult {
        self.in_string += 1;
        let r = self.run_text(text, true);
        self.in_string -= 1;
        r
    }

    /// [`Shell::run_string`], without expanding aliases if not `aliases`
    /// (for text that luish wrote, such as the startup cache's).
    pub fn run_text(&mut self, text: &[u8], aliases: bool) -> ExecResult {
        let lineno = self.lineno;
        let mut p = Parser::new(text.to_vec(), lineno.max(1), true);
        let mut status = 0;
        loop {
            let aliases = if aliases {
                self.aliases.clone()
            } else {
                crate::lexer::no_aliases()
            };
            p.bareglobqual = self.opt(Opt::Bareglobqual);
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
            status = crate::plugins::exit(self, status);
            crate::interactive::save_history(self);
            // Give the terminal back to whoever had it before us.
            self.set_jobctl(false);
        }
        sys::exit(status)
    }

    /// Runs the main input loop until end of input.
    pub fn run_input(&mut self, input: &mut Input) -> i32 {
        if let Some((text, command)) = input.whole_text() {
            self.run_whole(text, command)
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

    /// Runs a script or a `-c` command string. For `-c`, the last command
    /// may replace the shell process, as in dash.
    fn run_whole(&mut self, text: Vec<u8>, command: bool) -> i32 {
        let mut p = Parser::new(text, 1, true);
        loop {
            let start = p.consumed();
            let aliases = self.aliases.clone();
            p.bareglobqual = self.opt(Opt::Bareglobqual);
            match p.parse_next(&aliases) {
                Ok(Some(list)) => {
                    if self.opt(Opt::Verbose) {
                        let end = p.consumed().min(p.src.len());
                        sys::write_all(2, &p.src[start.min(end)..end]);
                    }
                    if !self.opt(Opt::Noexec) {
                        let r = self.run_list_exit(&list, command && p.at_end());
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
            p.bareglobqual = self.opt(Opt::Bareglobqual);
            match p.parse_next(&aliases) {
                Ok(Some(list)) => {
                    let used = p.consumed().min(buf.len());
                    let text: Vec<u8> = buf.drain(..used).collect();
                    lineno = p.lineno;
                    continuation = false;
                    self.job_warning = if self.job_warning == 2 { 1 } else { 0 };
                    if self.opt(Opt::Verbose) && !self.interactive {
                        sys::write_all(2, &text);
                    }
                    input.add_history(self, &text);
                    if self.interactive
                        && let Err(Flow::Exit(n)) = crate::plugins::preexec(self, &text)
                    {
                        self.exit(n);
                    }
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
                    if !continuation {
                        self.notify_jobs();
                        if self.interactive
                            && let Err(Flow::Exit(n)) = crate::plugins::precmd(self)
                        {
                            self.exit(n);
                        }
                    }
                    match input.read_line(self, continuation, &buf) {
                        Line::Text(line) => {
                            let line = match input.expand_history(self, &buf, line) {
                                Expanded::Line(line) => line,
                                Expanded::Again => continue,
                                Expanded::Drop => {
                                    lineno += buf.iter().filter(|&&c| c == b'\n').count() as u32;
                                    buf.clear();
                                    continuation = false;
                                    continue;
                                }
                            };
                            if self.interactive {
                                self.check_path_dirs();
                            }
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
                            if buf.is_empty() && self.stopped_jobs_warning() {
                                eof = false;
                            } else if self.interactive && buf.is_empty() && self.opt(Opt::Ignoreeof) {
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

fn readonly_message(name: &[u8]) -> String {
    format!("{}: is read only", String::from_utf8_lossy(name))
}
