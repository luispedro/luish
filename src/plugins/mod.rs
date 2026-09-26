//! Plugins (PLAN.md §6): the `plugin` built-in and the hooks that plugins
//! register. Nothing here costs anything until the first `plugin load`,
//! which creates the Rhai engine (`Shell::plugins`).

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

    fn prompt(&self, _: &mut Shell) -> Result<Option<Vec<u8>>, Flow> {
        match *self {}
    }
}

/// The events that plugins can hook.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookKind {
    /// The current directory changed (`cd`); called with the old and the
    /// new directory.
    Chpwd,
    /// Before each prompt: returns the prompt, used instead of `PS1`.
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    Prompt,
}

/// Runs the `chpwd` hooks after `cd` changed the directory.
pub fn chpwd(sh: &mut Shell, old: &[u8], new: &[u8]) -> Result<(), Flow> {
    match sh.plugins.clone() {
        None => Ok(()),
        Some(host) => host.run_hooks(sh, HookKind::Chpwd, &[old, new]),
    }
}

/// The prompt from the `prompt` hooks, if a plugin gives one.
pub fn prompt(sh: &mut Shell) -> Result<Option<Vec<u8>>, Flow> {
    match sh.plugins.clone() {
        None => Ok(None),
        Some(host) => host.prompt(sh),
    }
}

const USAGE: &str = "usage: plugin load NAME|PATH..., plugin list, plugin unload NAME...";

/// The `plugin` built-in (interactive shells only, like `help`).
pub fn plugin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// `plugin load NAME|PATH...`, `plugin list`, and `plugin unload NAME...`,
/// also available as `__luish_internal plugin`. `name` is the command, for
/// error messages.
///
/// `plugin restore NAME PATH`, which `savestate` prints, loads a plugin
/// under a name without running its `rc.lsh`, whose effects are in the
/// saved state.
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
        Some(b"list") if args.is_empty() => {
            let mut out = Vec::new();
            if let Some(host) = &sh.plugins {
                for name in host.names() {
                    out.extend_from_slice(&name);
                    out.push(b'\n');
                }
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

#[cfg(feature = "plugins")]
/// A plugin found on disk: a `.rhai` file or a directory.
struct Found {
    path: Vec<u8>,
    dir: bool,
}

#[cfg(feature = "plugins")]
fn is_dir(path: &[u8]) -> bool {
    crate::sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR)
}

#[cfg(feature = "plugins")]
/// The plugin for `plugin load ARG`: ARG itself if it contains a `/`,
/// otherwise `ARG.rhai` or else `ARG/` in the plugin directory.
fn find(sh: &Shell, arg: &[u8]) -> Option<Found> {
    if arg.contains(&b'/') {
        return Some(Found {
            path: arg.to_vec(),
            dir: is_dir(arg),
        });
    }
    let mut p = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    p.extend_from_slice(b"/luish/plugins/");
    p.extend_from_slice(arg);
    if is_dir(&p) && crate::sys::stat(&[p.as_slice(), b".rhai"].concat()).is_none() {
        return Some(Found { path: p, dir: true });
    }
    p.extend_from_slice(b".rhai");
    Some(Found { path: p, dir: false })
}

#[cfg(feature = "plugins")]
/// A plugin's name: its file name without `.rhai`, or its directory's name.
fn plugin_name(path: &[u8], dir: bool) -> Vec<u8> {
    let path = path.strip_suffix(b"/").unwrap_or(path);
    let base = path.rsplit(|&c| c == b'/').next().unwrap_or(path);
    match dir {
        true => base.to_vec(),
        false => base.strip_suffix(b".rhai").unwrap_or(base).to_vec(),
    }
}

/// A plugin being loaded, for `Host::load`.
#[cfg(feature = "plugins")]
pub struct Loading {
    pub name: Vec<u8>,
    /// The plugin's absolute path (its file or directory), for `savestate`.
    pub abs: Vec<u8>,
    /// The plugin's directory (absolute): the directory of a file plugin.
    pub dir: Vec<u8>,
    /// The Rhai file to run, as given (for messages) and absolute.
    pub rhai: Option<(Vec<u8>, Vec<u8>)>,
}

#[cfg(feature = "plugins")]
/// The variables set while a plugin's entry points run.
const PLUGIN_VARS: [&[u8]; 2] = [b"LUISH_PLUGIN_DIR", b"LUISH_PLUGIN_NAME"];

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
    let name = restore.clone().unwrap_or_else(|| plugin_name(&path, found.dir));
    let (loading, rc) = if found.dir {
        let entry = |f: &[u8]| {
            let p = [path.as_slice(), b"/", f].concat();
            crate::sys::stat(&p)
                .is_some()
                .then(|| (p, [abs.as_slice(), b"/", f].concat()))
        };
        let (rhai, rc) = (entry(b"plugin.rhai"), entry(b"rc.lsh"));
        if rhai.is_none() && rc.is_none() && entry(b"login.lsh").is_none() {
            sh.berr(
                cmd,
                format!(
                    "{}: not a plugin (no plugin.rhai, rc.lsh or login.lsh)",
                    String::from_utf8_lossy(&path)
                ),
            );
            return Ok(1);
        }
        let dir = abs.clone();
        (Loading { name, abs, dir, rhai }, rc.map(|(_, abs)| abs))
    } else {
        let dir = abs[..abs.iter().rposition(|&c| c == b'/').unwrap_or(0).max(1)].to_vec();
        let rhai = Some((path, abs.clone()));
        (Loading { name, abs, dir, rhai }, None)
    };
    // A changed plugin must invalidate the startup cache (`startcache.rs`).
    if let (Some(rec), Some((_, abs))) = (&mut sh.sourced_files, &loading.rhai) {
        rec.push(abs.clone());
    }
    let saved = PLUGIN_VARS.map(|v| sh.vars.take(v));
    for (v, value) in PLUGIN_VARS.iter().zip([&loading.dir, &loading.name]) {
        let var = crate::vars::Var {
            value: Some(value.clone()),
            ..Default::default()
        };
        sh.vars.restore(v, Some(var));
    }
    let host = sh.plugins.get_or_insert_with(|| std::rc::Rc::new(Host::new())).clone();
    let r = match (host.load(sh, cmd, loading), rc) {
        (Ok(0), Some(rc)) if restore.is_none() => crate::builtins::misc::dot(sh, &[b".".to_vec(), rc]).map(|_| 0),
        (r, _) => r,
    };
    for (v, var) in PLUGIN_VARS.iter().zip(saved) {
        sh.vars.restore(v, var);
    }
    r
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
    use super::plugin_name;

    #[test]
    fn names() {
        assert_eq!(plugin_name(b"/a/b/greet.rhai", false), b"greet");
        assert_eq!(plugin_name(b"./x", false), b"x");
        assert_eq!(plugin_name(b"greet.rhai", false), b"greet");
        assert_eq!(plugin_name(b"/a/b/greet/", true), b"greet");
        assert_eq!(plugin_name(b"dir.rhai", true), b"dir.rhai");
    }
}
