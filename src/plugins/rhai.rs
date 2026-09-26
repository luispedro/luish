//! The Rhai plugin host (PLAN.md §6.4): one engine, one AST per plugin, and
//! the `sh` module through which plugins reach the shell (and the `fs` and
//! `vcs` modules, `fs.rs` and `vcs.rs`).
//!
//! The `sh` functions reach the `Shell` through a pointer that is set for
//! the length of each call into Rhai (`enter`). Calls are re-entrant: a hook
//! can run shell code with `sh::run`, which can call into Rhai again. So no
//! borrow of the host's `RefCell`s is held across a call into Rhai.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;

use rhai::{AST, Dynamic, Engine, EvalAltResult, FnPtr, Module, ModuleResolver, Position, Scope, Shared};

use super::bytes::{to_bytes, to_str};
use super::{HookKind, Loading};
use crate::shell::{ExecResult, Flow, Shell};
use crate::{signals, sys};

pub(super) type RhaiResult<T> = Result<T, Box<EvalAltResult>>;

struct Plugin {
    id: u32,
    name: Vec<u8>,
    /// The Rhai file as given, for messages.
    path: Vec<u8>,
    /// The absolute path of the plugin (its file or directory), to load it
    /// again.
    abs: Vec<u8>,
    /// The plugin's directory (absolute), for `import` and `plugin_dir`.
    dir: Vec<u8>,
    /// `None` for a plugin without Rhai code (a directory with only
    /// `rc.lsh` or `login.lsh`).
    ast: Option<Rc<AST>>,
}

#[derive(Clone)]
struct Hook {
    plugin: u32,
    kind: HookKind,
    f: FnPtr,
    ast: Rc<AST>,
    path: Rc<[u8]>,
    /// Whether the function takes an argument beyond its curried ones:
    /// a `prompt` hook that does is given the previous prompt.
    takes_arg: bool,
}

pub struct Host {
    /// Created when the first Rhai code is loaded.
    engine: OnceCell<Engine>,
    plugins: RefCell<Vec<Plugin>>,
    hooks: RefCell<Vec<Hook>>,
    next_id: Cell<u32>,
    /// The hook kinds whose hooks are running, so that a hook doesn't
    /// trigger itself (a `chpwd` hook that runs `cd`).
    running: RefCell<Vec<HookKind>>,
    /// The modules that plugins imported, by absolute path. Emptied when a
    /// plugin is loaded, so that loading one again reads its modules again.
    modules: RefCell<HashMap<Vec<u8>, Shared<Module>>>,
}

thread_local! {
    /// The shell, while plugin code runs.
    static SHELL: Cell<*mut Shell> = const { Cell::new(std::ptr::null_mut()) };
    /// The plugin whose code is running (0 for none).
    static CURRENT: Cell<u32> = const { Cell::new(0) };
    /// Set when shell code run by a plugin exits the shell: plugin code
    /// stops, and the exit happens once it has.
    static EXIT: Cell<Option<i32>> = const { Cell::new(None) };
}

/// The values with which luish stops a script (`ErrorTerminated`), which
/// are not errors to report: the shell is exiting, or SIGINT arrived.
const EXITING: &str = "luish:exit";
const INTERRUPTED: &str = "luish:interrupt";

fn stop(why: &str) -> Box<EvalAltResult> {
    EvalAltResult::ErrorTerminated(why.into(), Position::NONE).into()
}

/// Why luish stopped the script, if it did.
fn stopped(e: &EvalAltResult) -> Option<String> {
    match e.unwrap_inner() {
        EvalAltResult::ErrorTerminated(v, _) => v.clone().into_string().ok(),
        _ => None,
    }
}

pub(super) fn error<T>(msg: impl Into<String>) -> RhaiResult<T> {
    Err(msg.into().into())
}

/// Runs `f`, which calls into Rhai for plugin `id`, with the shell
/// available to the `sh` functions. Returns `Err` if the shell must exit.
fn enter<R>(sh: &mut Shell, id: u32, f: impl FnOnce() -> R) -> Result<R, Flow> {
    let prev_sh = SHELL.replace(sh as *mut Shell);
    let prev_id = CURRENT.replace(id);
    let r = f();
    SHELL.set(prev_sh);
    CURRENT.set(prev_id);
    match EXIT.take() {
        Some(n) => Err(Flow::Exit(n)),
        None => Ok(r),
    }
}

/// Gives an `sh` function the shell.
pub(super) fn with_shell<R>(f: impl FnOnce(&mut Shell) -> RhaiResult<R>) -> RhaiResult<R> {
    let p = SHELL.get();
    if p.is_null() {
        return error("the shell is not available here");
    }
    // SAFETY: `enter` set the pointer from a `&mut Shell` that its caller
    // doesn't use until the call into Rhai returns, and resets it after.
    f(unsafe { &mut *p })
}

fn host(sh: &Shell) -> RhaiResult<Rc<Host>> {
    match &sh.plugins {
        Some(h) => Ok(h.clone()),
        None => error("no plugin host"),
    }
}

/// The directory of the plugin whose code is running.
fn current_dir() -> RhaiResult<Vec<u8>> {
    with_shell(|sh| {
        let host = host(sh)?;
        let plugins = host.plugins.borrow();
        match plugins.iter().find(|p| p.id == CURRENT.get()) {
            Some(p) => Ok(p.dir.clone()),
            None => error("no plugin is running"),
        }
    })
}

/// Resolves `import "NAME"` to `NAME.rhai` in the plugin's directory (or an
/// absolute path), whichever file or module does the import.
struct Resolver;

impl ModuleResolver for Resolver {
    fn resolve(&self, engine: &Engine, _: Option<&str>, name: &str, pos: Position) -> RhaiResult<Shared<Module>> {
        let mut path = match name.starts_with('/') {
            true => Vec::new(),
            false => [current_dir()?, b"/".to_vec()].concat(),
        };
        path.extend(to_bytes(name));
        path.extend_from_slice(b".rhai");
        let cached = with_shell(|sh| Ok(host(sh)?.modules.borrow().get(&path).cloned()))?;
        if let Some(m) = cached {
            return Ok(m);
        }
        let in_module = |e| Box::new(EvalAltResult::ErrorInModule(name.into(), e, pos));
        let text = match std::fs::read(std::ffi::OsStr::from_bytes(&path)) {
            Ok(t) => String::from_utf8(t).map_err(|_| in_module("not valid UTF-8".into()))?,
            Err(_) => return Err(EvalAltResult::ErrorModuleNotFound(name.into(), pos).into()),
        };
        let ast = engine.compile(&text).map_err(|e| in_module(e.into()))?;
        let m: Shared<Module> = Module::eval_ast_as_new(Scope::new(), &ast, engine)
            .map_err(in_module)?
            .into();
        with_shell(|sh| {
            host(sh)?.modules.borrow_mut().insert(path, m.clone());
            Ok(())
        })?;
        Ok(m)
    }
}

fn check_name(name: &str) -> RhaiResult<()> {
    let b = name.as_bytes();
    if b.first().is_some_and(|&c| crate::lexer::is_name_start(c)) && b.iter().all(|&c| crate::lexer::is_name_char(c)) {
        Ok(())
    } else {
        error(format!("{name}: bad variable name"))
    }
}

/// Converts a Rhai string to shell bytes that can't hold NUL (arguments,
/// variable values).
pub(super) fn to_shell(s: &str) -> RhaiResult<Vec<u8>> {
    let b = to_bytes(s);
    if b.contains(&0) {
        return error("string contains a NUL byte");
    }
    Ok(b)
}

fn hook_kind(name: &str) -> Option<HookKind> {
    match name {
        "chpwd" => Some(HookKind::Chpwd),
        "prompt" => Some(HookKind::Prompt),
        _ => None,
    }
}

fn sh_module() -> Module {
    let mut m = Module::new();
    m.set_native_fn("hook", |kind: &str, f: FnPtr| {
        let Some(kind) = hook_kind(kind) else {
            return error(format!("unknown hook: {kind}"));
        };
        with_shell(|sh| {
            let host = host(sh)?;
            let plugin = CURRENT.get();
            let (ast, path) = {
                let plugins = host.plugins.borrow();
                let Some((p, Some(ast))) = plugins.iter().find(|p| p.id == plugin).map(|p| (p, &p.ast)) else {
                    return error("no plugin is running");
                };
                (ast.clone(), Rc::from(p.path.as_slice()))
            };
            let n = f.curry().len() + 1;
            let takes_arg = ast
                .iter_functions()
                .any(|d| d.name == f.fn_name() && d.params.len() == n);
            host.hooks.borrow_mut().push(Hook {
                plugin,
                kind,
                f,
                ast,
                path,
                takes_arg,
            });
            Ok(())
        })
    });
    m.set_native_fn("getvar", |name: &str| {
        with_shell(|sh| Ok(sh.get_var(&to_bytes(name)).map_or(Dynamic::UNIT, |v| to_str(&v).into())))
    });
    m.set_native_fn("setvar", |name: &str, value: &str| {
        check_name(name)?;
        let value = to_shell(value)?;
        with_shell(|sh| sh.try_set_var(name.as_bytes(), value).or_else(error))
    });
    m.set_native_fn("export", |name: &str| {
        check_name(name)?;
        with_shell(|sh| {
            sh.vars.entry(name.as_bytes()).exported = true;
            Ok(())
        })
    });
    m.set_native_fn("unsetvar", |name: &str| {
        check_name(name)?;
        with_shell(|sh| {
            if sh.vars.unset(name.as_bytes()).is_err() {
                return error(format!("{name}: is read only"));
            }
            sh.var_changed(name.as_bytes());
            Ok(())
        })
    });
    m.set_native_fn("cwd", || {
        with_shell(|sh| {
            Ok(sh
                .curdir
                .clone()
                .or_else(sys::getcwd)
                .map_or_else(String::new, |d| to_str(&d)))
        })
    });
    m.set_native_fn("plugin_dir", || current_dir().map(|d| to_str(&d)));
    m.set_native_fn("last_status", || with_shell(|sh| Ok(sh.last_status as i64)));
    m.set_native_fn("interactive", || with_shell(|sh| Ok(sh.interactive)));
    m.set_native_fn("run", |script: &str| {
        let script = to_shell(script)?;
        with_shell(|sh| {
            let status = match sh.run_string(&script) {
                Ok(n) | Err(Flow::Error(n)) | Err(Flow::Return(n)) => n,
                Err(Flow::Break(_)) | Err(Flow::Continue(_)) => 0,
                Err(Flow::Exit(n)) => {
                    EXIT.set(Some(n));
                    return Err(stop(EXITING));
                }
            };
            sh.last_status = status;
            Ok(status as i64)
        })
    });
    m.set_native_fn("write", |fd: i64, text: &str| {
        if fd != 1 && fd != 2 {
            return error(format!("write: bad file descriptor {fd}"));
        }
        sys::write_all(fd as i32, &to_bytes(text));
        Ok(())
    });
    m
}

fn write_line(fd: i32, s: &str) {
    let mut b = to_bytes(s);
    b.push(b'\n');
    sys::write_all(fd, &b);
}

fn new_engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_static_module("sh", sh_module().into());
    engine.register_static_module("fs", super::fs::module().into());
    engine.register_static_module("vcs", super::vcs::module().into());
    engine.set_module_resolver(Resolver);
    engine.on_print(|s| write_line(1, s));
    engine.on_debug(|s, _, _| write_line(2, s));
    // Ctrl-C (or a trapped SIGINT) stops plugin code, as it would a
    // command. The pending signal is handled when the shell regains
    // control.
    engine.on_progress(|_| signals::is_pending(libc::SIGINT).then(|| INTERRUPTED.into()));
    // A buggy plugin gets an error rather than exhausting the stack or
    // memory.
    engine
        .set_max_call_levels(64)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(64 << 20)
        .set_max_array_size(1 << 20)
        .set_max_map_size(1 << 20);
    engine
}

impl Host {
    pub fn new() -> Host {
        Host {
            engine: OnceCell::new(),
            plugins: RefCell::new(Vec::new()),
            hooks: RefCell::new(Vec::new()),
            next_id: Cell::new(1),
            running: RefCell::new(Vec::new()),
            modules: RefCell::new(HashMap::new()),
        }
    }

    fn engine(&self) -> &Engine {
        self.engine.get_or_init(new_engine)
    }

    /// The names of the loaded plugins, in the order they were loaded.
    pub fn names(&self) -> Vec<Vec<u8>> {
        self.plugins.borrow().iter().map(|p| p.name.clone()).collect()
    }

    /// The loaded plugins' names and absolute paths, in the order they
    /// were loaded.
    pub fn loaded(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.plugins
            .borrow()
            .iter()
            .map(|p| (p.name.clone(), p.abs.clone()))
            .collect()
    }

    /// Reports an error from plugin code, unless luish stopped it, and
    /// returns the status for it.
    fn report(sh: &Shell, path: &[u8], e: &EvalAltResult) -> i32 {
        match stopped(e).as_deref() {
            Some(INTERRUPTED) => 128 + libc::SIGINT,
            Some(EXITING) => 1,
            _ => {
                // The innermost error, with its position: the call stack
                // only adds noise for a hook.
                sh.error(format!("{}: {}", String::from_utf8_lossy(path), e.unwrap_inner()));
                1
            }
        }
    }

    /// Loads (or reloads) a plugin: compiles its Rhai code, if it has
    /// any, and runs its top level, which registers its hooks.
    /// `cmd` is the command, for error messages.
    pub fn load(self: &Rc<Self>, sh: &mut Shell, cmd: &[u8], plugin: Loading) -> ExecResult {
        let Loading { name, abs, dir, rhai } = plugin;
        let Some((path, rhai_abs)) = rhai else {
            self.unload(&name);
            let id = self.next_id.get();
            self.next_id.set(id + 1);
            self.plugins.borrow_mut().push(Plugin {
                id,
                name,
                path: abs.clone(),
                abs,
                dir,
                ast: None,
            });
            return Ok(0);
        };
        let shown = String::from_utf8_lossy(&path).into_owned();
        let text = match std::fs::read(std::ffi::OsStr::from_bytes(&rhai_abs)) {
            Ok(t) => t,
            Err(e) => {
                let msg = match e.raw_os_error() {
                    Some(libc::ENOENT) => "No such file".to_string(),
                    Some(n) => sys::strerror(n),
                    None => e.to_string(),
                };
                sh.berr(cmd, format!("cannot open {shown}: {msg}"));
                return Ok(1);
            }
        };
        let Ok(text) = String::from_utf8(text) else {
            sh.berr(cmd, format!("{shown}: not valid UTF-8"));
            return Ok(1);
        };
        let ast = match self.engine().compile(&text) {
            Ok(a) => a,
            Err(e) => {
                sh.error(format!("{shown}: {e}"));
                return Ok(1);
            }
        };
        let ast = Rc::new(ast);
        self.unload(&name);
        self.modules.borrow_mut().clear();
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.plugins.borrow_mut().push(Plugin {
            id,
            name,
            path: path.clone(),
            abs,
            dir,
            ast: Some(ast.clone()),
        });
        let r = enter(sh, id, || self.engine().run_ast(&ast));
        match r {
            Ok(Ok(())) => Ok(0),
            Ok(Err(e)) => {
                self.remove(id);
                Ok(Self::report(sh, &path, &e))
            }
            Err(flow) => {
                self.remove(id);
                Err(flow)
            }
        }
    }

    fn remove(&self, id: u32) {
        self.plugins.borrow_mut().retain(|p| p.id != id);
        self.hooks.borrow_mut().retain(|h| h.plugin != id);
    }

    /// Unloads a plugin by name. Returns false if it isn't loaded.
    pub fn unload(&self, name: &[u8]) -> bool {
        let id = self.plugins.borrow().iter().find(|p| p.name == name).map(|p| p.id);
        match id {
            Some(id) => {
                self.remove(id);
                true
            }
            None => false,
        }
    }

    /// Runs the hooks of one kind. A failing hook is reported and the
    /// others still run. `$?` is kept.
    pub fn run_hooks(&self, sh: &mut Shell, kind: HookKind, args: &[&[u8]]) -> Result<(), Flow> {
        if self.running.borrow().contains(&kind) {
            return Ok(());
        }
        let hooks: Vec<Hook> = self.hooks.borrow().iter().filter(|h| h.kind == kind).cloned().collect();
        if hooks.is_empty() {
            return Ok(());
        }
        let args: Vec<Dynamic> = args.iter().map(|a| to_str(a).into()).collect();
        self.running.borrow_mut().push(kind);
        let saved = sh.last_status;
        let mut result = Ok(());
        for h in hooks {
            let r = enter(sh, h.plugin, || {
                h.f.call::<Dynamic>(self.engine(), &h.ast, args.clone())
            });
            match r {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    if Self::report(sh, &h.path, &e) != 1 {
                        break;
                    }
                }
                Err(flow) => {
                    result = Err(flow);
                    break;
                }
            }
        }
        sh.last_status = saved;
        self.running.borrow_mut().retain(|&k| k != kind);
        result
    }

    /// Runs the `prompt` hooks, from the most recently registered, until
    /// one returns a string, which is the prompt. A hook that returns `()`
    /// or fails (which is reported) leaves it to the ones before it. A hook
    /// that takes an argument is given the prompt of the ones before it (or
    /// `PS1`, parameter-expanded). `$?` is kept.
    pub fn prompt(&self, sh: &mut Shell) -> Result<Option<Vec<u8>>, Flow> {
        let kind = HookKind::Prompt;
        if self.running.borrow().contains(&kind) {
            return Ok(None);
        }
        let hooks: Vec<Hook> = self.hooks.borrow().iter().filter(|h| h.kind == kind).cloned().collect();
        if hooks.is_empty() {
            return Ok(None);
        }
        self.running.borrow_mut().push(kind);
        let saved = sh.last_status;
        let result = self.prompt_from(sh, &hooks, saved);
        sh.last_status = saved;
        self.running.borrow_mut().retain(|&k| k != kind);
        result
    }

    /// The prompt that `hooks` give, trying the last first, or `None` for
    /// `PS1`. `saved` is `$?`, which each hook sees.
    fn prompt_from(&self, sh: &mut Shell, hooks: &[Hook], saved: i32) -> Result<Option<Vec<u8>>, Flow> {
        for (i, h) in hooks.iter().enumerate().rev() {
            // What this hook returns replaces the earlier hooks' prompt,
            // and `()` keeps it.
            let prev = match h.takes_arg {
                false => None,
                true => Some(match self.prompt_from(sh, &hooks[..i], saved)? {
                    Some(p) => p,
                    None => {
                        sh.last_status = saved;
                        sh.param_expand_prompt(b"PS1")
                    }
                }),
            };
            let args: Vec<Dynamic> = prev.iter().map(|p| to_str(p).into()).collect();
            sh.last_status = saved;
            let r = enter(sh, h.plugin, || h.f.call::<Dynamic>(self.engine(), &h.ast, args))?;
            match r {
                Ok(v) if v.is_string() => {
                    return Ok(Some(to_bytes(&v.into_immutable_string().unwrap_or_default())));
                }
                Ok(v) if v.is_unit() => {}
                Ok(v) => {
                    let msg = format!("prompt hook returned {}, not a string", v.type_name());
                    sh.error(format!("{}: {msg}", String::from_utf8_lossy(&h.path)));
                }
                Err(e) => {
                    if Self::report(sh, &h.path, &e) != 1 {
                        return Ok(prev);
                    }
                }
            }
            if prev.is_some() {
                return Ok(prev);
            }
        }
        Ok(None)
    }
}
