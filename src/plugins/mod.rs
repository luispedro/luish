//! Plugins (DEVELOPING.md): the `plugin` built-in, which loads plugins (a
//! `.rhai` file, or a directory of Rhai and shell files), and the hooks
//! that their extensions (their Rhai code) register. Nothing here costs
//! anything until the first `plugin load`, which creates the host
//! (`Shell::plugins`); the Rhai engine waits for the first extension.

#[cfg(feature = "plugins")]
mod bytes;
#[cfg(feature = "plugins")]
mod fs;
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

/// Runs the completer for `words[0]`, given the words of the command and
/// the index of the one being completed.
pub fn complete(sh: &mut Shell, words: &[Vec<u8>], index: usize) -> Result<Completion, Flow> {
    match sh.plugins.clone() {
        None => Ok(Completion::Default),
        Some(host) => host.complete(sh, words, index),
    }
}

const USAGE: &str = "usage: plugin load NAME|PATH..., plugin list-loaded, plugin list-available, plugin unload NAME...";

/// The `plugin` built-in (interactive shells only, like `help`).
pub fn plugin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// `plugin load NAME|PATH...`, `plugin list-loaded`, `plugin list-available`
/// and `plugin unload NAME...`, also available as `__luish_internal plugin`. `name` is the command, for
/// error messages.
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
                _ => plugin_dir(sh).map_or_else(Vec::new, |dir| available_names(&dir)),
            };
            let mut out = Vec::new();
            for name in names {
                out.extend_from_slice(&name);
                out.push(b'\n');
            }
            Ok(sh.out_status(&out))
        }
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
struct Found {
    path: Vec<u8>,
    kind: Kind,
}

#[cfg(feature = "plugins")]
fn is_dir(path: &[u8]) -> bool {
    crate::sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR)
}

#[cfg(feature = "plugins")]
/// The plugin for `plugin load ARG`: ARG itself if it contains a `/` (a
/// file ending in `.lsh` is shell, any other file Rhai), otherwise the
/// first of `ARG.rhai`, `ARG.lsh` and `ARG/` in the plugin directory.
fn find(sh: &Shell, arg: &[u8]) -> Option<Found> {
    if arg.contains(&b'/') {
        let kind = match is_dir(arg) {
            true => Kind::Dir,
            false if arg.ends_with(b".lsh") => Kind::Lsh,
            false => Kind::Rhai,
        };
        return Some(Found {
            path: arg.to_vec(),
            kind,
        });
    }
    let mut p = plugin_dir(sh)?;
    p.push(b'/');
    p.extend_from_slice(arg);
    let lsh = [p.as_slice(), b".lsh"].concat();
    let rhai = [p.as_slice(), b".rhai"].concat();
    Some(if crate::sys::stat(&rhai).is_some() {
        Found {
            path: rhai,
            kind: Kind::Rhai,
        }
    } else if crate::sys::stat(&lsh).is_some() {
        Found {
            path: lsh,
            kind: Kind::Lsh,
        }
    } else if is_dir(&p) {
        Found {
            path: p,
            kind: Kind::Dir,
        }
    } else {
        Found {
            path: rhai,
            kind: Kind::Rhai,
        }
    })
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
            value: Some(value.to_vec()),
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
fn load(sh: &mut Shell, cmd: &[u8], arg: &[u8], restore: Option<Vec<u8>>) -> ExecResult {
    let Some(found) = find(sh, arg) else {
        sh.berr(cmd, "no plugin directory (HOME is not set)");
        return Ok(1);
    };
    let path = found.path.strip_suffix(b"/").unwrap_or(&found.path).to_vec();
    let abs = match (path.first(), &sh.curdir) {
        (Some(b'/'), _) | (_, None) => path.clone(),
        (_, Some(dir)) => crate::builtins::cd::canonicalize(&[dir.as_slice(), b"/", path.as_slice()].concat()),
    };
    let name = restore.clone().unwrap_or_else(|| plugin_name(&path, found.kind));
    let (loading, init, rc) = if found.kind == Kind::Dir {
        let entry = |f: &[u8]| {
            let p = [path.as_slice(), b"/", f].concat();
            crate::sys::stat(&p)
                .is_some()
                .then(|| (p, [abs.as_slice(), b"/", f].concat()))
        };
        let (init, rhai, rc) = (entry(b"init.lsh"), entry(b"extension.rhai"), entry(b"rc.lsh"));
        let prompt_vars = entry(b"prompt-vars.lsh").map(|(_, abs)| abs);
        if init.is_none() && rhai.is_none() && rc.is_none() && prompt_vars.is_none() && entry(b"login.lsh").is_none() {
            sh.berr(
                cmd,
                format!(
                    "{}: not a plugin (no init.lsh, extension.rhai, rc.lsh, prompt-vars.lsh or login.lsh)",
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
    let fresh = restore.is_none();
    let rc = rc.filter(|_| fresh && sh.opt(Opt::Interactive));
    let dot = |sh: &mut Shell, file: Vec<u8>| crate::builtins::misc::dot(sh, &[b".".to_vec(), file]).map(|_| 0);
    with_plugin_vars(sh, &dir, &name, |sh| {
        if let Some(init) = init.filter(|_| fresh) {
            dot(sh, init)?;
        }
        match (host.load(sh, cmd, loading), rc) {
            (Ok(0), Some(rc)) => dot(sh, rc),
            (r, _) => r,
        }
    })
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
