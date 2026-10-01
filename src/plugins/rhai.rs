//! The plugin host and the Rhai side of extensions (DEVELOPING.md): one
//! engine, one AST per extension, and the `sh` module through which
//! extensions reach the shell (and the `fs` and `vcs` modules, `fs.rs` and
//! `vcs.rs`).
//!
//! The `sh` functions reach the `Shell` through a pointer that is set for
//! the length of each call into Rhai (`enter`). Calls are re-entrant: a hook
//! can run shell code with `sh::run`, which can call into Rhai again. So no
//! borrow of the host's `RefCell`s is held across a call into Rhai.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rhai::{AST, Dynamic, Engine, EvalAltResult, FnPtr, Module, ModuleResolver, Position, Scope, Shared};

use super::bytes::{to_bytes, to_str};
use super::{HookKind, Loading};
use crate::expand::{pattern::Pattern, split::XChar};
use crate::interactive::{Candidate, Completion, DEFAULT_COMPLETER, Suffix};
use crate::prompt::Prompt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::vars::{Assoc, Value};
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
    /// The plugin's extension. `None` for a plugin without one (a
    /// directory with only shell files).
    ast: Option<Rc<AST>>,
    /// The plugin's `prompt-vars.lsh` (absolute), run before each prompt.
    prompt_vars: Option<Vec<u8>>,
}

/// A function registered by an extension.
#[derive(Clone)]
struct Callback {
    plugin: u32,
    f: FnPtr,
    ast: Rc<AST>,
    path: Rc<[u8]>,
    /// Whether the function takes an argument beyond its curried ones:
    /// a `prompt-rewrite` hook that does is given the previous prompt.
    takes_arg: bool,
}

/// How long a completer may run.
const COMPLETE_BUDGET: Duration = Duration::from_secs(2);

pub struct Host {
    /// Created when the first Rhai code is loaded.
    engine: OnceCell<Engine>,
    plugins: RefCell<Vec<Plugin>>,
    hooks: RefCell<Vec<(HookKind, Callback)>>,
    /// The completers, by command name.
    completers: RefCell<Vec<(Vec<u8>, Callback)>>,
    /// The built-ins that extensions registered, by name.
    builtins: RefCell<Vec<(Vec<u8>, Callback)>>,
    next_id: Cell<u32>,
    /// The hook kinds whose hooks are running, so that a hook doesn't
    /// trigger itself (a `chpwd` hook that runs `cd`).
    running: RefCell<Vec<HookKind>>,
    /// The modules that extensions imported, by absolute path. Emptied when
    /// a plugin is loaded, so that loading one again reads its modules
    /// again.
    modules: RefCell<HashMap<Vec<u8>, Shared<Module>>>,
}

thread_local! {
    /// The shell, while extension code runs.
    static SHELL: Cell<*mut Shell> = const { Cell::new(std::ptr::null_mut()) };
    /// The plugin whose extension is running (0 for none).
    static CURRENT: Cell<u32> = const { Cell::new(0) };
    /// Set when shell code run by an extension exits the shell: the
    /// extension stops, and the exit happens once it has.
    static EXIT: Cell<Option<i32>> = const { Cell::new(None) };
    /// When extension code that the user is waiting for must stop.
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The values with which luish stops a script (`ErrorTerminated`), which
/// are not errors to report: the shell is exiting, or SIGINT arrived.
const EXITING: &str = "luish:exit";
const INTERRUPTED: &str = "luish:interrupt";
const TIMEOUT: &str = "luish:timeout";

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

/// The file of `import "@SOURCE/PLUGIN/MODULE"` (`name`), without `.rhai`:
/// MODULE in the directory of the loaded plugin PLUGIN. Loaded plugins have
/// different names, so the plugin is found by its name alone, however it
/// was loaded; SOURCE is written as in `[dependencies]`.
fn other_plugin(name: &str, pos: Position) -> RhaiResult<Vec<u8>> {
    // (Not `ErrorModuleNotFound`, which Rhai replaces with its own, without
    // the reason.)
    let not_found = |why: String| EvalAltResult::ErrorRuntime(format!("import {name}: {why}").into(), pos).into();
    let parts: Vec<&str> = name[1..].splitn(3, '/').collect();
    let [source, plugin, module] = parts[..] else {
        return Err(not_found("write @SOURCE/PLUGIN/MODULE".into()));
    };
    if [source, plugin, module].contains(&"") {
        return Err(not_found("write @SOURCE/PLUGIN/MODULE".into()));
    }
    let dir = with_shell(|sh| {
        let host = host(sh)?;
        let plugins = host.plugins.borrow();
        Ok(plugins
            .iter()
            .find(|p| p.name == to_bytes(plugin))
            .map(|p| p.dir.clone()))
    })?;
    match dir {
        Some(dir) => Ok([dir, b"/".to_vec(), to_bytes(module)].concat()),
        None => Err(not_found(format!(
            "the plugin {plugin} is not loaded (add {source}.{plugin} to [dependencies] in plugin.toml)"
        ))),
    }
}

/// Resolves `import "NAME"` to `NAME.rhai` in the directory of the file
/// that does the import (or an absolute path, or `@SOURCE/PLUGIN/MODULE`,
/// in another plugin). That file is the source of the code running
/// (`AST::set_source`, which Rhai also gives the functions and closures
/// defined in it, and `Module::set_id`), an absolute path; for code without
/// one, it is the plugin's directory.
struct Resolver;

impl ModuleResolver for Resolver {
    fn resolve(&self, engine: &Engine, source: Option<&str>, name: &str, pos: Position) -> RhaiResult<Shared<Module>> {
        let mut path = match (name.as_bytes().first(), source.filter(|s| s.starts_with('/'))) {
            (Some(b'@'), _) => other_plugin(name, pos)?,
            (Some(b'/'), _) => to_bytes(name),
            (_, Some(file)) => {
                let file = to_bytes(file);
                let dir = &file[..file.iter().rposition(|&c| c == b'/').unwrap_or(0) + 1];
                [dir, &to_bytes(name)].concat()
            }
            (_, None) => [current_dir()?, b"/".to_vec(), to_bytes(name)].concat(),
        };
        path.extend_from_slice(b".rhai");
        // (`import "../x"`: the same file is the same module.)
        let path = crate::builtins::cd::canonicalize(&path);
        let cached = with_shell(|sh| Ok(host(sh)?.modules.borrow().get(&path).cloned()))?;
        if let Some(m) = cached {
            return Ok(m);
        }
        let in_module = |e| Box::new(EvalAltResult::ErrorInModule(name.into(), e, pos));
        let text = match std::fs::read(std::ffi::OsStr::from_bytes(&path)) {
            Ok(t) => String::from_utf8(t).map_err(|_| in_module("not valid UTF-8".into()))?,
            Err(_) => return Err(EvalAltResult::ErrorModuleNotFound(name.into(), pos).into()),
        };
        let mut ast = engine.compile(&text).map_err(|e| in_module(e.into()))?;
        ast.set_source(to_str(&path));
        let mut m = Module::eval_ast_as_new(Scope::new(), &ast, engine).map_err(in_module)?;
        // The source of calls to its functions (`m::f()`).
        m.set_id(to_str(&path));
        let m: Shared<Module> = m.into();
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

/// An element of an array or map given to `setvar`, which must be a string.
fn shell_string(v: &Dynamic) -> RhaiResult<Vec<u8>> {
    match v.read_lock::<rhai::ImmutableString>() {
        Some(s) => to_shell(&s),
        None => error(format!("setvar: an element can't be {}", v.type_name())),
    }
}

fn hook_kind(name: &str) -> Option<HookKind> {
    match name {
        "chpwd" => Some(HookKind::Chpwd),
        "prompt-vars" => Some(HookKind::PromptVars),
        "prompt-rewrite" => Some(HookKind::PromptRewrite),
        "post-rc" => Some(HookKind::PostRc),
        _ => None,
    }
}

/// Registers `f` for the running plugin with `add`.
fn register(f: FnPtr, add: impl FnOnce(&Host, Callback)) -> RhaiResult<()> {
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
        add(
            &host,
            Callback {
                plugin,
                f,
                ast,
                path,
                takes_arg,
            },
        );
        Ok(())
    })
}

/// `sh::capture_sh(script)`: runs `script` in a subshell, as `$(...)` does,
/// and returns its status and its output without trailing newlines.
fn capture_sh(sh: &mut Shell, script: &[u8]) -> RhaiResult<rhai::Map> {
    match super::capture(sh, script) {
        Ok((status, out)) => Ok(captured(status, out, None)),
        Err(e) => error(format!("capture_sh: {e}")),
    }
}

/// `sh::capture(argv, stderr)`: runs the program `argv[0]` with the
/// arguments `argv[1..]` (see [`super::capture_argv`]) and returns its
/// status and its output, and with `stderr` "return", its standard error,
/// without trailing newlines.
fn capture(argv: rhai::Array, stderr: &str) -> RhaiResult<rhai::Map> {
    use super::Stderr;
    let mode = match stderr {
        "discard" => Stderr::Discard,
        "inherit" => Stderr::Inherit,
        "merge" => Stderr::Merge,
        "return" => Stderr::Return,
        _ => {
            return error(format!(
                r#"capture: stderr is "discard", "inherit", "merge" or "return", not "{stderr}""#
            ));
        }
    };
    if argv.is_empty() {
        return error("capture: the array is empty (it holds the program and its arguments)");
    }
    let mut words = Vec::with_capacity(argv.len());
    for w in argv {
        let Ok(w) = w.into_immutable_string() else {
            return error("capture: the array must hold strings");
        };
        words.push(to_shell(&w)?);
    }
    with_shell(|sh| match super::capture_argv(sh, &words, mode) {
        Ok((status, out, err)) => Ok(captured(status, out, (mode == Stderr::Return).then_some(err))),
        Err(e) => error(format!("capture: {e}")),
    })
}

/// `#{status, out}`, and `err` if there is one, with NUL bytes and trailing
/// newlines removed, as `$(...)` removes them.
fn captured(status: i32, out: Vec<u8>, err: Option<Vec<u8>>) -> rhai::Map {
    let text = |mut b: Vec<u8>| {
        b.retain(|&c| c != 0);
        while b.last() == Some(&b'\n') {
            b.pop();
        }
        to_str(&b)
    };
    let mut m = rhai::Map::new();
    m.insert("status".into(), (status as i64).into());
    m.insert("out".into(), text(out).into());
    if let Some(err) = err {
        m.insert("err".into(), text(err).into());
    }
    m
}

/// `sh::read_line()`: a line from fd 0 without its newline, or `()` at end
/// of file. As the `read` built-in, it reads a byte at a time, so that
/// what follows the line is left for the next command.
fn read_line() -> RhaiResult<Dynamic> {
    let mut line = Vec::new();
    let mut buf = [0u8; 1];
    loop {
        match sys::read(0, &mut buf, true) {
            Ok(1) if buf[0] == b'\n' => break,
            Ok(1) => line.push(buf[0]),
            Err(libc::EINTR) if !signals::is_pending(libc::SIGINT) => {}
            Err(libc::EINTR) => return Err(stop(INTERRUPTED)),
            _ if line.is_empty() => return Ok(Dynamic::UNIT),
            _ => break,
        }
    }
    Ok(to_str(&line).into())
}

/// The message of a string thrown (with `throw`, or by an `sh` function),
/// as shell bytes.
fn thrown(e: &EvalAltResult) -> Option<Vec<u8>> {
    match e.unwrap_inner() {
        EvalAltResult::ErrorRuntime(v, _) => v.read_lock::<rhai::ImmutableString>().map(|s| to_bytes(&s)),
        _ => None,
    }
}

/// The exit status for what an extension's built-in returned: `()` is 0,
/// a boolean true 0 and false 1, and an integer is taken modulo 256, as
/// `return` does.
fn builtin_status(v: Dynamic) -> Result<i32, String> {
    if v.is_unit() {
        Ok(0)
    } else if let Some(b) = v.clone().try_cast::<bool>() {
        Ok(!b as i32)
    } else if let Some(n) = v.clone().try_cast::<i64>() {
        Ok((n & 0xff) as i32)
    } else {
        Err(format!("returned {}, not a status", v.type_name()))
    }
}

/// Converts what a completer returned for the word `word`: `()` for the
/// default completion, or an array of strings and of maps with a `value`
/// and an optional `desc` and `suffix`, or a map with such an array
/// (`candidates`) and the start of the word they leave alone (`prefix`).
/// Gives the candidates and the length of the prefix.
fn candidates(r: Dynamic, word: &[u8]) -> RhaiResult<Option<(usize, Vec<Candidate>)>> {
    if r.is_unit() {
        return Ok(None);
    }
    let (prefix, r) = match r.try_cast_result::<rhai::Map>() {
        Ok(mut m) => {
            let prefix = match m.remove("prefix") {
                None => Vec::new(),
                Some(p) => match p.into_immutable_string() {
                    Ok(p) => to_bytes(&p),
                    Err(_) => return error("a completer's prefix must be a string"),
                },
            };
            if !word.starts_with(&prefix) {
                return error("a completer's prefix must begin the word");
            }
            (prefix.len(), m.remove("candidates").unwrap_or_default())
        }
        Err(r) => (0, r),
    };
    let Some(items) = r.try_cast::<rhai::Array>() else {
        return error("a completer must return an array, a map or ()");
    };
    let string = |d: &Dynamic, what: &str| match d.clone().into_immutable_string() {
        Ok(s) => Ok(to_bytes(&s)),
        Err(_) => error(format!("a candidate's {what} must be a string")),
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if item.is_string() {
            out.push(Candidate::word(&string(&item, "value")?));
            continue;
        }
        let Some(m) = item.try_cast::<rhai::Map>() else {
            return error("a candidate must be a string or a map");
        };
        let Some(value) = m.get("value") else {
            return error("a candidate has no value");
        };
        let mut c = Candidate::word(&string(value, "value")?);
        if let Some(d) = m.get("desc") {
            c.desc = Some(string(d, "desc")?);
        }
        if let Some(s) = m.get("suffix") {
            let s = string(s, "suffix")?;
            c.suffix = if s.is_empty() { Suffix::None } else { Suffix::Close(s) };
        }
        out.push(c);
    }
    Ok(Some((prefix, out)))
}

fn sh_module() -> Module {
    let mut m = Module::new();
    m.set_native_fn("hook", |kind: &str, f: FnPtr| {
        let Some(kind) = hook_kind(kind) else {
            return error(format!("unknown hook: {kind}"));
        };
        register(f, |host, cb| host.hooks.borrow_mut().push((kind, cb)))
    });
    m.set_native_fn("completer", |command: &str, f: FnPtr| {
        let command = to_bytes(command);
        register(f, |host, cb| {
            let mut completers = host.completers.borrow_mut();
            completers.retain(|c| c.0 != command);
            completers.push((command, cb));
        })
    });
    m.set_native_fn("builtin", |name: &str, f: FnPtr| {
        let name = to_shell(name)?;
        if name.is_empty() || name.contains(&b'/') {
            return error(format!("builtin: {}: bad command name", to_str(&name)));
        }
        if crate::builtins::names().any(|b| b == name) {
            return error(format!("builtin: {}: is a shell builtin", to_str(&name)));
        }
        register(f, |host, cb| {
            let mut builtins = host.builtins.borrow_mut();
            builtins.retain(|b| b.0 != name);
            builtins.push((name, cb));
        })
    });
    m.set_native_fn("read_line", read_line);
    m.set_native_fn("capture", |argv: rhai::Array| capture(argv, "discard"));
    m.set_native_fn("capture", capture);
    m.set_native_fn("capture", |_: &str| -> RhaiResult<rhai::Map> {
        error("capture: takes an array, the program and its arguments (sh::capture_sh runs shell code)")
    });
    m.set_native_fn("which", |name: &str| {
        let name = to_shell(name)?;
        with_shell(|sh| Ok(sh.which(&name).map_or(Dynamic::UNIT, |p| to_str(&p).into())))
    });
    m.set_native_fn("commands", |prefix: &str| {
        let prefix = to_shell(prefix)?;
        with_shell(|sh| {
            let path = sh.get_var(b"PATH").unwrap_or_default();
            Ok(crate::path::executables(&path, &prefix)
                .iter()
                .map(|n| Dynamic::from(to_str(n)))
                .collect::<rhai::Array>())
        })
    });
    m.set_native_fn("capture_sh", |script: &str| {
        let script = to_shell(script)?;
        with_shell(|sh| capture_sh(sh, &script))
    });
    m.set_native_fn("quote", |s: &str| {
        Ok(to_str(&crate::builtins::single_quote(&to_bytes(s))))
    });
    m.set_native_fn("quote", |words: rhai::Array| {
        let mut out = Vec::new();
        for (i, w) in words.into_iter().enumerate() {
            let Ok(w) = w.into_immutable_string() else {
                return error("quote: the array must hold strings");
            };
            if i > 0 {
                out.push(b' ');
            }
            out.extend(crate::builtins::single_quote(&to_bytes(&w)));
        }
        Ok(to_str(&out))
    });
    m.set_native_fn("expand_prompt", |text: &str| {
        let text = to_shell(text)?;
        with_shell(|sh| Ok(to_str(&crate::prompt::expand(sh, &text).text)))
    });
    m.set_native_fn("matches", |pattern: &str, s: &str| {
        // As `case $s in $pattern)`: unquoted, so a backslash escapes.
        let p: Vec<XChar> = to_bytes(pattern)
            .into_iter()
            .map(|b| XChar { b, quoted: false })
            .collect();
        Ok(Pattern::new(&p).matches(&to_bytes(s)))
    });
    m.set_native_fn("getvar", |name: &str| {
        with_shell(|sh| Ok(sh.get_var(&to_bytes(name)).map_or(Dynamic::UNIT, |v| to_str(&v).into())))
    });
    m.set_native_fn("setvar", |name: &str, value: &str| {
        check_name(name)?;
        let value = to_shell(value)?;
        with_shell(|sh| sh.try_set_var(name.as_bytes(), value).or_else(error))
    });
    m.set_native_fn("getarray", |name: &str| {
        let name = to_bytes(name);
        with_shell(|sh| {
            let elements = match sh.vars.get_value(&name) {
                Some(v) => v.elements().to_vec(),
                None => match sh.special_elements(&name) {
                    Some(e) => e,
                    None => return Ok(Dynamic::UNIT),
                },
            };
            Ok(elements
                .iter()
                .map(|e| Dynamic::from(to_str(e)))
                .collect::<rhai::Array>()
                .into())
        })
    });
    m.set_native_fn("getmap", |name: &str| {
        with_shell(|sh| {
            let Some(Value::Assoc(h)) = sh.vars.get_value(&to_bytes(name)) else {
                return Ok(Dynamic::UNIT);
            };
            let map: rhai::Map = h
                .keys()
                .iter()
                .zip(h.values())
                .map(|(k, v)| (to_str(k).into(), to_str(v).into()))
                .collect();
            Ok(map.into())
        })
    });
    m.set_native_fn("setvar", |name: &str, elements: rhai::Array| {
        check_name(name)?;
        let elements = elements.iter().map(shell_string).collect::<RhaiResult<Vec<_>>>()?;
        let value = Value::Array(Box::new(elements));
        with_shell(|sh| sh.try_set_var_value(name.as_bytes(), value).or_else(error))
    });
    m.set_native_fn("setvar", |name: &str, map: rhai::Map| {
        check_name(name)?;
        let mut h = Assoc::default();
        for (k, v) in &map {
            h.insert(&to_shell(k)?, shell_string(v)?);
        }
        let value = Value::Assoc(Box::new(h));
        with_shell(|sh| sh.try_set_var_value(name.as_bytes(), value).or_else(error))
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

/// Sets the variables that a `prompt-vars` hook (from the file `path`)
/// returned: a map of names to strings, numbers or booleans, or to `()` to
/// unset one; or `()` for none. A bad value or variable is reported and
/// skipped.
fn set_prompt_vars(sh: &mut Shell, path: &[u8], v: Dynamic) {
    if v.is_unit() {
        return;
    }
    let fail = |sh: &Shell, msg: String| sh.error(format!("{}: {msg}", String::from_utf8_lossy(path)));
    let map = match v.try_cast_result::<rhai::Map>() {
        Ok(map) => map,
        Err(v) => return fail(sh, format!("prompt-vars hook returned {}, not a map", v.type_name())),
    };
    for (name, value) in map {
        let value = match value {
            v if v.is_unit() => None,
            v if v.is_string() => Some(to_bytes(&v.into_immutable_string().unwrap_or_default())),
            v if v.is_int() || v.is_float() || v.is_bool() || v.is_char() => Some(v.to_string().into_bytes()),
            v => {
                fail(sh, format!("{name}: a prompt variable can't be {}", v.type_name()));
                continue;
            }
        };
        let r = match value {
            _ if !crate::lexer::is_valid_name(name.as_bytes()) => Err(format!("{name}: bad variable name")),
            Some(value) if value.contains(&0) => Err(format!("{name}: string contains a NUL byte")),
            Some(value) => sh.try_set_var(name.as_bytes(), value),
            None => match sh.vars.unset(name.as_bytes()) {
                Ok(()) => {
                    sh.var_changed(name.as_bytes());
                    Ok(())
                }
                Err(_) => Err(format!("{name}: is read only")),
            },
        };
        if let Err(msg) = r {
            fail(sh, msg);
        }
    }
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
    // `eval` compiles and runs a string, which would bypass the source and
    // id that `import` resolves relative to.
    engine.disable_symbol("eval");
    engine.on_print(|s| write_line(1, s));
    engine.on_debug(|s, _, _| write_line(2, s));
    // Ctrl-C (or a trapped SIGINT) stops extension code, as it would a
    // command. The pending signal is handled when the shell regains
    // control.
    // Code that the user waits for, such as a completer, also stops
    // when its time is up.
    // Rhai interns short strings, and once its cache is full each new one
    // scans the whole cache: that made a built-in that returns a different
    // short string each call about a fifth slower, and a long-lived shell
    // would get there anyway.
    engine.set_max_strings_interned(0);
    engine.on_progress(|ops| {
        if signals::is_pending(libc::SIGINT) {
            Some(INTERRUPTED.into())
        } else if ops % 1024 == 0 && DEADLINE.get().is_some_and(|d| Instant::now() >= d) {
            Some(TIMEOUT.into())
        } else {
            None
        }
    });
    // A buggy extension gets an error rather than exhausting the stack or
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
            completers: RefCell::new(Vec::new()),
            builtins: RefCell::new(Vec::new()),
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

    /// Reports an error from extension code, unless luish stopped it, and
    /// returns the status for it.
    fn report(sh: &Shell, path: &[u8], e: &EvalAltResult) -> i32 {
        match stopped(e).as_deref() {
            Some(INTERRUPTED) => 128 + libc::SIGINT,
            Some(EXITING) => 1,
            Some(TIMEOUT) => {
                sh.error(format!("{}: took too long", String::from_utf8_lossy(path)));
                1
            }
            _ => {
                // The innermost error, with its position: the call stack
                // only adds noise for a hook.
                sh.error(format!("{}: {}", String::from_utf8_lossy(path), e.unwrap_inner()));
                1
            }
        }
    }

    /// Loads (or reloads) a plugin: compiles its extension, if it has
    /// one, and runs its top level, which registers its hooks.
    /// `cmd` is the command, for error messages.
    pub fn load(self: &Rc<Self>, sh: &mut Shell, cmd: &[u8], plugin: Loading) -> ExecResult {
        let Loading {
            name,
            abs,
            dir,
            rhai,
            prompt_vars,
        } = plugin;
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
                prompt_vars,
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
        let mut ast = match self.engine().compile(&text) {
            Ok(a) => a,
            Err(e) => {
                sh.error(format!("{shown}: {e}"));
                return Ok(1);
            }
        };
        // For `import` (`Resolver`).
        ast.set_source(to_str(&rhai_abs));
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
            prompt_vars,
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
        self.hooks.borrow_mut().retain(|h| h.1.plugin != id);
        self.completers.borrow_mut().retain(|c| c.1.plugin != id);
        self.builtins.borrow_mut().retain(|b| b.1.plugin != id);
    }

    /// Unloads a plugin by name. Returns false if it isn't loaded.
    pub fn unload(&self, name: &[u8]) -> bool {
        self.unload_if(|p| p.name == name)
    }

    /// Unloads the plugin at `abs` (an absolute path), if it is loaded.
    pub fn unload_path(&self, abs: &[u8]) -> bool {
        self.unload_if(|p| p.abs == abs)
    }

    fn unload_if(&self, pred: impl Fn(&Plugin) -> bool) -> bool {
        let id = self.plugins.borrow().iter().find(|p| pred(p)).map(|p| p.id);
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
        self.run_hooks_of(sh, kind, args, None)
    }

    /// Runs the hooks of one kind that the plugin `name` registered.
    pub fn run_plugin_hooks(&self, sh: &mut Shell, kind: HookKind, name: &[u8]) -> Result<(), Flow> {
        let id = self.plugins.borrow().iter().find(|p| p.name == name).map(|p| p.id);
        match id {
            Some(id) => self.run_hooks_of(sh, kind, &[], Some(id)),
            None => Ok(()),
        }
    }

    /// Runs the hooks of one kind, of all plugins or of the plugin `only`.
    fn run_hooks_of(&self, sh: &mut Shell, kind: HookKind, args: &[&[u8]], only: Option<u32>) -> Result<(), Flow> {
        if self.running.borrow().contains(&kind) {
            return Ok(());
        }
        let hooks: Vec<Callback> = self
            .hooks
            .borrow()
            .iter()
            .filter(|h| h.0 == kind && only.is_none_or(|id| h.1.plugin == id))
            .map(|h| h.1.clone())
            .collect();
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

    /// The hooks of one kind, in the order they were registered.
    fn hooks_of(&self, kind: HookKind) -> Vec<Callback> {
        (self.hooks.borrow().iter())
            .filter(|h| h.0 == kind)
            .map(|h| h.1.clone())
            .collect()
    }

    /// Builds the `PS1` prompt, and with `right` the right prompt
    /// (`RPROMPT`), or returns `None` if no extension has a `prompt-vars`
    /// or `prompt-rewrite` hook and no plugin a `prompt-vars.lsh`.
    ///
    /// In this order: the `prompt-vars` hooks and files set variables
    /// (`prompt_vars`); the `prompt-rewrite` hooks, which see them, give
    /// the prompt (`prompt_from`), or else `PS1` is expanded with them;
    /// then the variables that the first step changed are put back. Each
    /// hook and file sees the `$?` of the last command, which is kept.
    pub fn prompt(&self, sh: &mut Shell, right: bool) -> Result<Option<(Prompt, Option<Prompt>)>, Flow> {
        let kinds = [HookKind::PromptVars, HookKind::PromptRewrite];
        if self.running.borrow().iter().any(|k| kinds.contains(k)) {
            return Ok(None);
        }
        let rewrite = self.hooks_of(HookKind::PromptRewrite);
        let vars = self.hooks.borrow().iter().any(|h| h.0 == HookKind::PromptVars)
            || self.plugins.borrow().iter().any(|p| p.prompt_vars.is_some());
        if rewrite.is_empty() && !vars {
            return Ok(None);
        }
        self.running.borrow_mut().extend(kinds);
        let saved = sh.last_status;
        let saved_lineno = sh.lineno;
        // Only the changes made by `prompt-vars` are undone: what a
        // `prompt-rewrite` hook sets stays, as it did before there were
        // prompt variables.
        let (result, changed) = match vars {
            false => (Ok(()), Vec::new()),
            true => {
                let snapshot = sh.vars.snapshot();
                let r = self.prompt_vars(sh, saved);
                (r, sh.vars.changes_since(&snapshot))
            }
        };
        let result = result.and_then(|()| {
            let text = self.prompt_from(sh, &rewrite, saved)?;
            sh.last_status = saved;
            let left = match text {
                Some(text) => sh.percent_expand_prompt(text),
                None => sh.prompt(b"PS1"),
            };
            Ok(Some((left, right.then(|| sh.right_prompt(false)).flatten())))
        });
        for (name, var) in changed {
            sh.restore_var(name, var);
        }
        sh.last_status = saved;
        sh.lineno = saved_lineno;
        self.running.borrow_mut().retain(|k| !kinds.contains(k));
        result
    }

    /// Runs the `prompt-vars` hooks and `prompt-vars.lsh` files, plugin by
    /// plugin in the order they were loaded (a plugin's hooks before its
    /// file), so that each sees and can override the variables of the
    /// ones before it. A hook returns a map of variables to set (`()` as a
    /// value unsets one), or `()`. Errors are reported and the rest still
    /// run. `saved` is `$?`, which each sees.
    fn prompt_vars(&self, sh: &mut Shell, saved: i32) -> Result<(), Flow> {
        let plugins: Vec<_> = (self.plugins.borrow().iter())
            .map(|p| (p.id, p.dir.clone(), p.name.clone(), p.prompt_vars.clone()))
            .collect();
        let hooks = self.hooks_of(HookKind::PromptVars);
        for (id, dir, name, file) in plugins {
            for h in hooks.iter().filter(|h| h.plugin == id) {
                sh.last_status = saved;
                let r = enter(sh, h.plugin, || h.f.call::<Dynamic>(self.engine(), &h.ast, ()))?;
                match r {
                    Ok(v) => set_prompt_vars(sh, &h.path, v),
                    Err(e) => {
                        if Self::report(sh, &h.path, &e) != 1 {
                            return Ok(());
                        }
                    }
                }
            }
            if let Some(file) = file {
                sh.last_status = saved;
                let r = super::with_plugin_vars(sh, &dir, &name, |sh| {
                    crate::builtins::misc::dot(sh, &[b".".to_vec(), file])
                });
                match r {
                    Err(Flow::Exit(n)) => return Err(Flow::Exit(n)),
                    // Ctrl-C stops the rest, as it would a list of commands.
                    _ if signals::is_pending(libc::SIGINT) => return Ok(()),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The prompt that the `prompt-rewrite` hooks `hooks` give, trying the
    /// last first, or `None` for `PS1`. A hook that returns `()` or fails
    /// (which is reported) leaves it to the ones before it. A hook that
    /// takes an argument is given the prompt of the ones before it (or
    /// `PS1`, parameter-expanded). `saved` is `$?`, which each hook sees.
    fn prompt_from(&self, sh: &mut Shell, hooks: &[Callback], saved: i32) -> Result<Option<Vec<u8>>, Flow> {
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
                    let msg = format!("prompt-rewrite hook returned {}, not a string", v.type_name());
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

    /// Whether an extension registered the built-in `name`.
    pub fn has_builtin(&self, name: &[u8]) -> bool {
        self.builtins.borrow().iter().any(|b| b.0 == name)
    }

    /// The names of the extensions' built-ins.
    pub fn builtin_names(&self) -> Vec<Vec<u8>> {
        self.builtins.borrow().iter().map(|b| b.0.clone()).collect()
    }

    /// The name of the plugin whose extension registered the built-in
    /// `name`.
    pub fn builtin_plugin(&self, name: &[u8]) -> Option<Vec<u8>> {
        let id = self.builtins.borrow().iter().find(|b| b.0 == name)?.1.plugin;
        self.plugins
            .borrow()
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
    }

    /// Runs the extension's built-in `argv[0]`, which is given `argv` as an
    /// array. A string thrown (or an error from an `sh` function) is
    /// reported as the built-in's, `name: message`, and other errors with
    /// the extension's file and position; either way the status is 1.
    pub fn run_builtin(&self, sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
        let found = self
            .builtins
            .borrow()
            .iter()
            .find(|b| b.0 == argv[0])
            .map(|b| b.1.clone());
        let Some(cb) = found else {
            sh.berr(&argv[0], "not found");
            return Ok(127);
        };
        let args: rhai::Array = argv.iter().map(|a| to_str(a).into()).collect();
        let r = enter(sh, cb.plugin, || cb.f.call::<Dynamic>(self.engine(), &cb.ast, (args,)))?;
        match r {
            Ok(v) => Ok(builtin_status(v).unwrap_or_else(|msg| {
                sh.error(format!("{}: {msg}", String::from_utf8_lossy(&cb.path)));
                1
            })),
            Err(e) => match thrown(&e) {
                Some(msg) => {
                    sh.berr(&argv[0], String::from_utf8_lossy(&msg));
                    Ok(1)
                }
                None => Ok(Self::report(sh, &cb.path, &e)),
            },
        }
    }

    /// The commands that have completers.
    pub fn completer_names(&self) -> Vec<Vec<u8>> {
        self.completers.borrow().iter().map(|c| c.0.clone()).collect()
    }

    /// Runs the completer for `words[0]`, or else the default completer, if
    /// there is one, with the words of the command and the index of the one
    /// being completed. An error is reported on a line of its own, below
    /// the command line in an interactive shell. `$?` is kept.
    pub fn complete(&self, sh: &mut Shell, words: &[Vec<u8>], index: usize) -> Result<Completion, Flow> {
        let cb = {
            let completers = self.completers.borrow();
            let find = |name: &[u8]| completers.iter().find(|c| c.0 == name);
            match find(&words[0]).or_else(|| find(DEFAULT_COMPLETER)) {
                Some(c) => c.1.clone(),
                None => return Ok(Completion::Default),
            }
        };
        let array: rhai::Array = words.iter().map(|w| to_str(w).into()).collect();
        let args = (array, index as i64);
        let saved = sh.last_status;
        let deadline = DEADLINE.replace(Some(Instant::now() + COMPLETE_BUDGET));
        let r = enter(sh, cb.plugin, || cb.f.call::<Dynamic>(self.engine(), &cb.ast, args));
        DEADLINE.set(deadline);
        sh.last_status = saved;
        match r?.and_then(|r| candidates(r, &words[index])) {
            Ok(Some((prefix, c))) => Ok(Completion::Candidates(prefix, c)),
            Ok(None) => Ok(Completion::Default),
            Err(e) => {
                // Below the command line (not for `__luish_internal
                // complete` in a script).
                if sh.opt(crate::options::Opt::Interactive) {
                    sys::write_all(2, b"\n");
                }
                Self::report(sh, &cb.path, &e);
                Ok(Completion::Failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a script might return from a completer.
    fn values() -> Vec<Dynamic> {
        let map = |pairs: &[(&str, Dynamic)]| -> Dynamic {
            let mut m = rhai::Map::new();
            for (k, v) in pairs {
                m.insert((*k).into(), v.clone());
            }
            m.into()
        };
        let strs: Dynamic = vec![Dynamic::from("a"), Dynamic::from("")].into();
        let odd: Dynamic = vec![
            Dynamic::from(1_i64),
            Dynamic::UNIT,
            Dynamic::from(true),
            Dynamic::from(1.5_f64),
        ]
        .into();
        let cand = |v: Dynamic, d: Dynamic, s: Dynamic| -> Dynamic { map(&[("value", v), ("desc", d), ("suffix", s)]) };
        let mut out = vec![
            Dynamic::UNIT,
            Dynamic::from(1_i64),
            Dynamic::from("a"),
            Dynamic::from('c'),
            Dynamic::from(true),
            strs.clone(),
            odd.clone(),
            vec![map(&[]), map(&[("value", "x".into())]), map(&[("value", 1_i64.into())])].into(),
            vec![cand("v".into(), "d".into(), "]}".into())].into(),
            vec![cand("v".into(), 1_i64.into(), "".into())].into(),
            vec![cand("v".into(), Dynamic::UNIT, 2_i64.into())].into(),
            map(&[]),
        ];
        for prefix in [
            Dynamic::from(""),
            "a".into(),
            "--x=".into(),
            "\u{10FF80}".into(),
            1_i64.into(),
            Dynamic::UNIT,
        ] {
            for list in [Dynamic::UNIT, strs.clone(), odd.clone(), 1_i64.into()] {
                out.push(map(&[("prefix", prefix.clone()), ("candidates", list)]));
            }
            out.push(map(&[("prefix", prefix)]));
        }
        out
    }

    #[test]
    fn completer_results_never_panic() {
        let words: [&[u8]; 5] = [b"", b"a", b"--x=1", b"\xff", b"\xf4\x8f\xbe\x80"];
        for v in values() {
            for w in words {
                let _ = candidates(v.clone(), w);
            }
        }
    }

    #[test]
    fn completer_prefix_must_begin_the_word() {
        let mut m = rhai::Map::new();
        m.insert("prefix".into(), "zz".into());
        assert!(candidates(m.into(), b"a").is_err());
        let mut m = rhai::Map::new();
        m.insert("prefix".into(), "a".into());
        m.insert("candidates".into(), vec![Dynamic::from("ab")].into());
        let (n, c) = candidates(m.into(), b"ab").unwrap().unwrap();
        assert_eq!((n, c.len()), (1, 1));
    }
}
