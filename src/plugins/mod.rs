//! Plugins (DEVELOPING.md): the `plugin` built-in, which loads plugins (a
//! `.rhai` file, or a directory of Rhai and shell files), and the hooks
//! that their extensions (their Rhai code) register. Nothing here costs
//! anything until the first `plugin load`, which creates the host
//! (`Shell::plugins`); the Rhai engine waits for the first extension.

#[cfg(feature = "plugins")]
mod bytes;
#[cfg(feature = "plugins")]
mod fetch;
#[cfg(feature = "plugins")]
mod fs;
#[cfg(feature = "plugins")]
mod package;
#[cfg(feature = "plugins")]
mod rhai;
#[cfg(feature = "plugins")]
mod vcs;

#[cfg(feature = "plugins")]
pub use rhai::Host;

use crate::interactive::Completion;
#[cfg(feature = "plugins")]
use crate::options::Opt;
use crate::prompt::Prompt;
use crate::shell::{ExecResult, Flow, Shell};

/// Without plugin support there is never a host.
#[cfg(not(feature = "plugins"))]
pub enum Host {}

#[cfg(not(feature = "plugins"))]
impl Host {
    fn names(&self) -> Vec<Vec<u8>> {
        match *self {}
    }

    pub fn loaded(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        match *self {}
    }

    fn unload(&self, _: &[u8]) -> bool {
        match *self {}
    }

    fn run_hooks(&self, _: &mut Shell, _: HookKind, _: &[&[u8]]) -> Result<(), Flow> {
        match *self {}
    }

    fn prompt(&self, _: &mut Shell) -> Result<Option<Prompt>, Flow> {
        match *self {}
    }

    fn completer_names(&self) -> Vec<Vec<u8>> {
        match *self {}
    }

    pub fn has_builtin(&self, _: &[u8]) -> bool {
        match *self {}
    }

    fn builtin_names(&self) -> Vec<Vec<u8>> {
        match *self {}
    }

    fn builtin_plugin(&self, _: &[u8]) -> Option<Vec<u8>> {
        match *self {}
    }

    fn run_builtin(&self, _: &mut Shell, _: &[Vec<u8>]) -> ExecResult {
        match *self {}
    }

    fn complete(&self, _: &mut Shell, _: &[Vec<u8>], _: usize) -> Result<Completion, Flow> {
        match *self {}
    }
}

/// The events that extensions can hook.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookKind {
    /// The current directory changed (`cd`); called with the old and the
    /// new directory.
    Chpwd,
    /// Before each prompt: returns variables to set while `PS1` is
    /// expanded.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    PromptVars,
    /// Before each prompt, after `PromptVars`: returns the prompt, used
    /// instead of `PS1`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    PromptRewrite,
    /// In interactive shells, once after the startup files of `rc.d` (or,
    /// for a plugin loaded later, right after it is loaded).
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    PostRc,
}

/// Runs the `chpwd` hooks after `cd` changed the directory.
pub fn chpwd(sh: &mut Shell, old: &[u8], new: &[u8]) -> Result<(), Flow> {
    match sh.plugins.clone() {
        None => Ok(()),
        Some(host) => host.run_hooks(sh, HookKind::Chpwd, &[old, new]),
    }
}

/// The `PS1` prompt, built with the extensions' `prompt-vars` and
/// `prompt-rewrite` hooks and the plugins' `prompt-vars.lsh` files, or
/// `None` if there are none.
pub fn prompt(sh: &mut Shell) -> Result<Option<Prompt>, Flow> {
    match sh.plugins.clone() {
        None => Ok(None),
        Some(host) => host.prompt(sh),
    }
}

/// The commands for which extensions provide completers.
pub fn completer_names(sh: &Shell) -> Vec<Vec<u8>> {
    sh.plugins.as_ref().map_or_else(Vec::new, |host| host.completer_names())
}

/// The names of the built-ins that extensions registered.
pub fn builtin_names(sh: &Shell) -> Vec<Vec<u8>> {
    sh.plugins.as_ref().map_or_else(Vec::new, |host| host.builtin_names())
}

/// The name of the plugin that added the built-in `name`, for `type`.
pub fn builtin_plugin(sh: &Shell, name: &[u8]) -> Option<Vec<u8>> {
    sh.plugins.as_ref()?.builtin_plugin(name)
}

/// Runs the extension's built-in `argv[0]` (see [`Shell::lookup_command`]).
pub fn run_builtin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    match sh.plugins.clone() {
        None => Ok(127),
        Some(host) => host.run_builtin(sh, argv),
    }
}

/// Runs the completer for `words[0]`, given the words of the command and
/// the index of the one being completed.
pub fn complete(sh: &mut Shell, words: &[Vec<u8>], index: usize) -> Result<Completion, Flow> {
    match sh.plugins.clone() {
        None => Ok(Completion::Default),
        Some(host) => host.complete(sh, words, index),
    }
}

const USAGE: &str = "usage: plugin load NAME|PATH..., plugin list-loaded, plugin list-available, plugin unload NAME..., \
                     plugin sync [-q], plugin update [-q] [SOURCE...], plugin check";

/// The `plugin` built-in (interactive shells only, like `help`).
pub fn plugin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// `plugin load NAME|PATH...`, `plugin list-loaded`, `plugin list-available`,
/// `plugin unload NAME...`, `plugin sync [-q]`, `plugin update [-q]
/// [SOURCE...]` and `plugin check`, also available as `__luish_internal
/// plugin`. `name` is the command, for error messages.
///
/// `plugin restore NAME PATH`, which `savestate` prints, loads a plugin
/// under a name without running its shell files (`init.lsh`, `rc.lsh`),
/// whose effects are in the saved state.
pub fn run(sh: &mut Shell, name: &[u8], argv: &[Vec<u8>]) -> ExecResult {
    let args = argv.get(1..).unwrap_or_default();
    match argv.first().map(|a| a.as_slice()) {
        Some(b"load") if !args.is_empty() => {
            let mut status = 0;
            for a in args {
                if !sh.no_plugins {
                    status = load(sh, name, a, None)?.max(status);
                }
            }
            Ok(status)
        }
        Some(b"restore") if args.len() == 2 => {
            if sh.no_plugins {
                return Ok(0);
            }
            load(sh, name, &args[1], Some(args[0].clone()))
        }
        Some(sub @ (b"list-loaded" | b"list-available")) if args.is_empty() => {
            let names = match sub {
                b"list-loaded" => loaded_names(sh),
                _ => not_loaded(sh),
            };
            let mut out = Vec::new();
            for name in names {
                out.extend_from_slice(&name);
                out.push(b'\n');
            }
            Ok(sh.out_status(&out))
        }
        Some(sub @ (b"sync" | b"update")) => {
            let quiet = args
                .iter()
                .take_while(|a| matches!(a.as_slice(), b"-q" | b"--quiet"))
                .count();
            match (sub, &args[quiet..]) {
                (b"sync", []) => sync(sh, name, None, quiet > 0),
                (b"update", names) => sync(sh, name, Some(names), quiet > 0),
                _ => {
                    sh.berr(name, USAGE);
                    Ok(2)
                }
            }
        }
        Some(b"check") if args.is_empty() => check(sh, name),
        Some(b"unload") if !args.is_empty() => {
            let mut status = 0;
            for a in args {
                if !unload(sh, a) {
                    sh.berr(name, format!("{}: not loaded", String::from_utf8_lossy(a)));
                    status = 1;
                }
            }
            Ok(status)
        }
        _ => {
            sh.berr(name, USAGE);
            Ok(2)
        }
    }
}

/// The names of the loaded plugins.
pub fn loaded_names(sh: &Shell) -> Vec<Vec<u8>> {
    sh.plugins.as_ref().map_or_else(Vec::new, |host| host.names())
}

/// The directory where `plugin load` finds plugins by name.
pub fn plugin_dir(sh: &Shell) -> Option<Vec<u8>> {
    let mut p = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    p.extend_from_slice(b"/luish/plugins");
    Some(p)
}

/// The names of the plugins in `dir` (the plugin directory), sorted: its
/// `.rhai` and `.lsh` files without the suffix, and its directories.
pub fn available_names(dir: &[u8]) -> Vec<Vec<u8>> {
    let mut names: Vec<_> = crate::sys::read_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|n| match n.strip_suffix(b".rhai").or_else(|| n.strip_suffix(b".lsh")) {
            Some(base) => Some(base.to_vec()),
            None => crate::sys::stat(&[dir, b"/", &n].concat())
                .is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR)
                .then_some(n),
        })
        .filter(|n| !n.is_empty() && !n.starts_with(b"."))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(feature = "plugins")]
/// The kinds of plugin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A directory of entry points (`init.lsh`, `extension.rhai`, ...).
    Dir,
    /// `NAME.rhai`: a directory with only `extension.rhai`.
    Rhai,
    /// `NAME.lsh`: a directory with only `init.lsh`.
    Lsh,
}

#[cfg(feature = "plugins")]
/// A plugin found on disk.
#[derive(Clone, Debug)]
struct Found {
    path: Vec<u8>,
    kind: Kind,
}

#[cfg(feature = "plugins")]
fn is_dir(path: &[u8]) -> bool {
    crate::sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR)
}

#[cfg(feature = "plugins")]
/// The files that make a directory a plugin.
const ENTRY_POINTS: [&[u8]; 6] = [
    b"init.lsh",
    b"extension.rhai",
    b"rc.lsh",
    b"post-rc.lsh",
    b"prompt-vars.lsh",
    b"login.lsh",
];

#[cfg(feature = "plugins")]
/// Whether the directory `dir` is a plugin (has one of the entry points),
/// rather than a collection of plugins.
fn is_plugin_dir(dir: &[u8]) -> bool {
    ENTRY_POINTS
        .iter()
        .any(|f| crate::sys::stat(&[dir, b"/", f].concat()).is_some())
}

#[cfg(feature = "plugins")]
/// The plugin `name` in the collection `dir` (such as the plugin
/// directory): the first of `NAME.rhai`, `NAME.lsh` and `NAME/`.
fn find_in(dir: &[u8], name: &[u8]) -> Option<Found> {
    let p = [dir, b"/", name].concat();
    let rhai = [p.as_slice(), b".rhai"].concat();
    let lsh = [p.as_slice(), b".lsh"].concat();
    if crate::sys::stat(&rhai).is_some() {
        Some(Found {
            path: rhai,
            kind: Kind::Rhai,
        })
    } else if crate::sys::stat(&lsh).is_some() {
        Some(Found {
            path: lsh,
            kind: Kind::Lsh,
        })
    } else if is_dir(&p) {
        Some(Found {
            path: p,
            kind: Kind::Dir,
        })
    } else {
        None
    }
}

#[cfg(feature = "plugins")]
/// The plugin at `path`, a file or a directory: a file whose name ends in
/// `.lsh` is shell, any other file Rhai.
fn at_path(path: &[u8]) -> Found {
    let kind = match is_dir(path) {
        true => Kind::Dir,
        false if path.ends_with(b".lsh") => Kind::Lsh,
        false => Kind::Rhai,
    };
    Found {
        path: path.to_vec(),
        kind,
    }
}

#[cfg(feature = "plugins")]
/// The plugin for `plugin load ARG`: ARG itself if it contains a `/`,
/// otherwise the first of `ARG.rhai`, `ARG.lsh` and `ARG/` in the plugin
/// directory (or `ARG.rhai`, which doesn't exist, for the error).
fn find(sh: &Shell, arg: &[u8]) -> Option<Found> {
    if arg.contains(&b'/') {
        return Some(at_path(arg));
    }
    let dir = plugin_dir(sh)?;
    Some(find_in(&dir, arg).unwrap_or_else(|| Found {
        path: [dir.as_slice(), b"/", arg, b".rhai"].concat(),
        kind: Kind::Rhai,
    }))
}

#[cfg(feature = "plugins")]
/// A plugin's name: its file name without `.rhai` or `.lsh`, or its
/// directory's name.
fn plugin_name(path: &[u8], kind: Kind) -> Vec<u8> {
    let path = path.strip_suffix(b"/").unwrap_or(path);
    let base = path.rsplit(|&c| c == b'/').next().unwrap_or(path);
    let suffix: &[u8] = match kind {
        Kind::Dir => b"",
        Kind::Rhai => b".rhai",
        Kind::Lsh => b".lsh",
    };
    base.strip_suffix(suffix).unwrap_or(base).to_vec()
}

/// A plugin being loaded, for `Host::load`.
#[cfg(feature = "plugins")]
pub struct Loading {
    pub name: Vec<u8>,
    /// The plugin's absolute path (its file or directory), for `savestate`.
    pub abs: Vec<u8>,
    /// The plugin's directory (absolute): the directory of a file plugin.
    pub dir: Vec<u8>,
    /// The extension to run (its Rhai file), as given (for messages) and
    /// absolute.
    pub rhai: Option<(Vec<u8>, Vec<u8>)>,
    /// The plugin's `prompt-vars.lsh` (absolute), if it has one.
    pub prompt_vars: Option<Vec<u8>>,
}

#[cfg(feature = "plugins")]
/// The variables set while a plugin's entry points run.
const PLUGIN_VARS: [&[u8]; 2] = [b"LUISH_PLUGIN_DIR", b"LUISH_PLUGIN_NAME"];

#[cfg(feature = "plugins")]
/// Runs `f` with `LUISH_PLUGIN_DIR` and `LUISH_PLUGIN_NAME` set to `dir`
/// and `name`, and puts them back afterwards.
pub(super) fn with_plugin_vars<R>(sh: &mut Shell, dir: &[u8], name: &[u8], f: impl FnOnce(&mut Shell) -> R) -> R {
    let saved = PLUGIN_VARS.map(|v| sh.vars.take(v));
    for (v, value) in PLUGIN_VARS.iter().zip([dir, name]) {
        let var = crate::vars::Var {
            value: Some(crate::vars::Value::Str(value.to_vec())),
            ..Default::default()
        };
        sh.vars.restore(v.to_vec(), Some(var));
    }
    let r = f(sh);
    for (v, var) in PLUGIN_VARS.iter().zip(saved) {
        sh.vars.restore(v.to_vec(), var);
    }
    r
}

#[cfg(feature = "plugins")]
/// `plugin load ARG`, or `plugin restore NAME ARG` (with `restore`, the
/// name).
fn load(sh: &mut Shell, cmd: &[u8], arg: &[u8], restore: Option<Vec<u8>>) -> ExecResult {
    if restore.is_none() {
        return package::load(sh, cmd, arg);
    }
    let Some(found) = find(sh, arg) else {
        sh.berr(cmd, "no plugin directory (HOME is not set)");
        return Ok(1);
    };
    load_found(sh, cmd, &found, restore, false)
}

#[cfg(feature = "plugins")]
/// The absolute form of a path, from the current directory.
fn absolute(sh: &Shell, path: &[u8]) -> Vec<u8> {
    let path = path.strip_suffix(b"/").unwrap_or(path);
    match (path.first(), &sh.curdir) {
        (Some(b'/'), _) | (_, None) => path.to_vec(),
        (_, Some(dir)) => crate::builtins::cd::canonicalize(&[dir.as_slice(), b"/", path].concat()),
    }
}

#[cfg(feature = "plugins")]
/// Loads the plugin `found`, under the name `name` (by default, from its
/// path). `fresh` is false when restoring a saved state, which already has
/// what the plugin's shell files did.
fn load_found(sh: &mut Shell, cmd: &[u8], found: &Found, name: Option<Vec<u8>>, fresh: bool) -> ExecResult {
    let path = found.path.strip_suffix(b"/").unwrap_or(&found.path).to_vec();
    let abs = absolute(sh, &path);
    let name = name.unwrap_or_else(|| plugin_name(&path, found.kind));
    let (loading, init, rc) = if found.kind == Kind::Dir {
        let entry = |f: &[u8]| {
            let p = [path.as_slice(), b"/", f].concat();
            crate::sys::stat(&p)
                .is_some()
                .then(|| (p, [abs.as_slice(), b"/", f].concat()))
        };
        let (init, rhai, rc) = (entry(b"init.lsh"), entry(b"extension.rhai"), entry(b"rc.lsh"));
        let prompt_vars = entry(b"prompt-vars.lsh").map(|(_, abs)| abs);
        if !is_plugin_dir(&path) {
            sh.berr(
                cmd,
                format!(
                    "{}: not a plugin (no init.lsh, extension.rhai, rc.lsh, post-rc.lsh, prompt-vars.lsh or login.lsh)",
                    String::from_utf8_lossy(&path)
                ),
            );
            return Ok(1);
        }
        let dir = abs.clone();
        let loading = Loading {
            name,
            abs,
            dir,
            rhai,
            prompt_vars,
        };
        (loading, init.map(|(_, abs)| abs), rc.map(|(_, abs)| abs))
    } else {
        if found.kind == Kind::Lsh && crate::sys::stat(&path).is_none() {
            sh.berr(
                cmd,
                format!("cannot open {}: No such file", String::from_utf8_lossy(&path)),
            );
            return Ok(1);
        }
        let dir = abs[..abs.iter().rposition(|&c| c == b'/').unwrap_or(0).max(1)].to_vec();
        let (rhai, init) = match found.kind {
            Kind::Lsh => (None, Some(abs.clone())),
            _ => (Some((path, abs.clone())), None),
        };
        let loading = Loading {
            name,
            abs,
            dir,
            rhai,
            prompt_vars: None,
        };
        (loading, init, None)
    };
    // A changed plugin must invalidate the startup cache (`startcache.rs`).
    // (`.` records the shell files it runs.)
    if let (Some(rec), Some((_, abs))) = (&mut sh.sourced_files, &loading.rhai) {
        rec.push(abs.clone());
    }
    let host = sh.plugins.get_or_insert_with(|| std::rc::Rc::new(Host::new())).clone();
    let (dir, name) = (loading.dir.clone(), loading.name.clone());
    // The shell files' effects are in the saved state when restoring, and
    // `rc.lsh` is only for interactive shells (and their subshells).
    let interactive = fresh && sh.opt(Opt::Interactive);
    let rc = rc.filter(|_| interactive);
    // During `rc.d`, `post-rc.lsh` and the `post-rc` hooks wait for its end
    // (`post_rc_files`, `post_rc_hooks`).
    let post_rc = interactive && !sh.in_rc;
    let is_dir = found.kind == Kind::Dir;
    // `plugin.toml`'s options, aliases and key bindings are for interactive
    // shells, as `rc.lsh`, and come just before it.
    let manifest = is_dir.then(|| [dir.as_slice(), b"/plugin.toml"].concat());
    if let (Some(rec), Some(file), true) = (&mut sh.sourced_files, &manifest, fresh)
        && !rec.contains(file)
    {
        rec.push(file.clone());
    }
    let manifest = manifest.filter(|_| interactive);
    with_plugin_vars(sh, &dir, &name, |sh| {
        if let Some(init) = init.filter(|_| fresh) {
            dot(sh, init)?;
        }
        let r = match host.load(sh, cmd, loading) {
            Ok(0) => {
                if let Some(manifest) = manifest {
                    crate::config::load_plugin_manifest(sh, &manifest);
                }
                rc.map_or(Ok(0), |rc| dot(sh, rc))
            }
            r => r,
        };
        match r {
            Ok(0) if post_rc => {
                let file = [dir.as_slice(), b"/post-rc.lsh"].concat();
                if is_dir && crate::sys::stat(&file).is_some() {
                    dot(sh, file)?;
                }
                host.run_plugin_hooks(sh, HookKind::PostRc, &name).map(|_| 0)
            }
            r => r,
        }
    })
}

#[cfg(feature = "plugins")]
fn dot(sh: &mut Shell, file: Vec<u8>) -> ExecResult {
    crate::builtins::misc::dot(sh, &[b".".to_vec(), file]).map(|_| 0)
}

/// Runs the `post-rc.lsh` of each directory plugin loaded, in the order
/// they were loaded, after the files of `rc.d` (in its cache).
#[cfg(feature = "plugins")]
pub fn post_rc_files(sh: &mut Shell) {
    let Some(host) = sh.plugins.clone() else {
        return;
    };
    for (name, abs) in host.loaded() {
        let file = [abs.as_slice(), b"/post-rc.lsh"].concat();
        if !is_dir(&abs) || crate::sys::stat(&file).is_none() {
            continue;
        }
        let r = with_plugin_vars(sh, &abs, &name, |sh| dot(sh, file));
        if let Err(Flow::Exit(n)) = r {
            sh.exit(n);
        }
    }
}

#[cfg(not(feature = "plugins"))]
pub fn post_rc_files(_: &mut Shell) {}

/// Runs the `post-rc` hooks, after `rc.d` (cached or not).
pub fn post_rc_hooks(sh: &mut Shell) {
    if let Some(host) = sh.plugins.clone()
        && let Err(Flow::Exit(n)) = host.run_hooks(sh, HookKind::PostRc, &[])
    {
        sh.exit(n);
    }
}

#[cfg(feature = "plugins")]
/// Runs `script` in a subshell, as `$(...)` does, and returns its status
/// and its output. Its standard error is the shell's.
fn capture(sh: &mut Shell, script: &[u8]) -> Result<(i32, Vec<u8>), &'static str> {
    use crate::sys;
    let Ok((r, w)) = sys::pipe() else {
        return Err("cannot create a pipe");
    };
    let pid = match sh.fork_or_error() {
        Ok(pid) => pid,
        Err(_) => {
            sys::close(r);
            sys::close(w);
            return Err("cannot fork");
        }
    };
    if pid == 0 {
        sys::close(r);
        let _ = sys::dup2(w, 1);
        sys::close(w);
        let res = sh.run_string(script);
        sh.child_exit(res);
    }
    sys::close(w);
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n) = sys::read(r, &mut buf, false) {
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    sys::close(r);
    Ok((sh.wait_for(pid), out))
}

/// The plugins of the sources in `config.toml` that are installed.
/// For `plugin list-available`: the plugins in the plugin directory and
/// those of the sources that are installed, less those loaded (by path, as
/// `std/NAME` loads as `NAME`).
#[cfg(feature = "plugins")]
fn not_loaded(sh: &mut Shell) -> Vec<Vec<u8>> {
    let mut found: Vec<_> = match plugin_dir(sh) {
        Some(dir) => (available_names(&dir).into_iter())
            .filter_map(|n| Some((find_in(&dir, &n)?.path, n)))
            .collect(),
        None => Vec::new(),
    };
    found.extend(package::available(sh));
    let loaded: Vec<_> = sh.plugins.as_ref().map_or_else(Vec::new, |host| host.loaded());
    (found.into_iter())
        .filter(|(path, _)| {
            let abs = absolute(sh, path);
            !loaded.iter().any(|(_, p)| *p == abs)
        })
        .map(|(_, name)| name)
        .collect()
}

#[cfg(not(feature = "plugins"))]
fn not_loaded(sh: &mut Shell) -> Vec<Vec<u8>> {
    plugin_dir(sh).map_or_else(Vec::new, |dir| available_names(&dir))
}

/// `plugin sync` and `plugin update`.
#[cfg(feature = "plugins")]
fn sync(sh: &mut Shell, cmd: &[u8], update: Option<&[Vec<u8>]>, quiet: bool) -> ExecResult {
    if sh.no_plugins {
        return Ok(0);
    }
    package::sync(sh, cmd, update, quiet)
}

#[cfg(not(feature = "plugins"))]
fn sync(sh: &mut Shell, cmd: &[u8], _: Option<&[Vec<u8>]>, _: bool) -> ExecResult {
    sh.berr(cmd, "luish was built without plugin support");
    Ok(1)
}

/// `plugin check`.
#[cfg(feature = "plugins")]
fn check(sh: &mut Shell, cmd: &[u8]) -> ExecResult {
    if sh.no_plugins {
        return Ok(0);
    }
    package::check(sh, cmd)
}

#[cfg(not(feature = "plugins"))]
fn check(sh: &mut Shell, cmd: &[u8]) -> ExecResult {
    sh.berr(cmd, "luish was built without plugin support");
    Ok(1)
}

/// Loads the plugins that `config.toml` enables, at the start of an
/// interactive shell. Returns false if some couldn't be found, so that the
/// startup cache isn't written.
#[cfg(feature = "plugins")]
pub fn load_enabled(sh: &mut Shell) -> bool {
    sh.no_plugins || package::load_enabled(sh)
}

#[cfg(not(feature = "plugins"))]
pub fn load_enabled(_: &mut Shell) -> bool {
    true
}

#[cfg(not(feature = "plugins"))]
fn load(sh: &mut Shell, name: &[u8], arg: &[u8], _: Option<Vec<u8>>) -> ExecResult {
    sh.berr(
        name,
        format!(
            "{}: luish was built without plugin support",
            String::from_utf8_lossy(arg)
        ),
    );
    Ok(1)
}

/// `plugin unload ARG`: the plugin loaded under the name ARG, else the one
/// that `plugin load ARG` would load (so `std/NAME` unloads `NAME`).
#[cfg(feature = "plugins")]
fn unload(sh: &mut Shell, arg: &[u8]) -> bool {
    let Some(host) = sh.plugins.clone() else {
        return false;
    };
    if host.unload(arg) {
        return true;
    }
    let path = match package::location(sh, arg) {
        Some(path) => path,
        None => match find(sh, arg) {
            Some(found) => found.path,
            None => return false,
        },
    };
    host.unload_path(&absolute(sh, &path))
}

#[cfg(not(feature = "plugins"))]
fn unload(sh: &mut Shell, name: &[u8]) -> bool {
    sh.plugins.as_ref().is_some_and(|host| host.unload(name))
}

#[cfg(all(test, feature = "plugins"))]
mod tests {
    use super::{Kind, plugin_name};

    #[test]
    fn names() {
        assert_eq!(plugin_name(b"/a/b/greet.rhai", Kind::Rhai), b"greet");
        assert_eq!(plugin_name(b"./x", Kind::Rhai), b"x");
        assert_eq!(plugin_name(b"greet.rhai", Kind::Rhai), b"greet");
        assert_eq!(plugin_name(b"/a/b/greet.lsh", Kind::Lsh), b"greet");
        assert_eq!(plugin_name(b"greet.lsh", Kind::Rhai), b"greet.lsh");
        assert_eq!(plugin_name(b"/a/b/greet/", Kind::Dir), b"greet");
        assert_eq!(plugin_name(b"dir.rhai", Kind::Dir), b"dir.rhai");
    }
}
