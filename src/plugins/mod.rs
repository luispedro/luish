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
pub fn run(sh: &mut Shell, name: &[u8], argv: &[Vec<u8>]) -> ExecResult {
    let args = argv.get(1..).unwrap_or_default();
    match argv.first().map(|a| a.as_slice()) {
        Some(b"load") if !args.is_empty() => {
            let mut status = 0;
            for a in args {
                if !sh.no_plugins {
                    status = load(sh, name, a)?.max(status);
                }
            }
            Ok(status)
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

/// The file for `plugin load ARG`: ARG itself if it contains a `/`,
/// otherwise `ARG.rhai` in the plugin directory.
fn plugin_path(sh: &Shell, arg: &[u8]) -> Option<Vec<u8>> {
    if arg.contains(&b'/') {
        return Some(arg.to_vec());
    }
    let mut p = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    p.extend_from_slice(b"/luish/plugins/");
    p.extend_from_slice(arg);
    p.extend_from_slice(b".rhai");
    Some(p)
}

/// A plugin's name: its file name without `.rhai`.
fn plugin_name(path: &[u8]) -> Vec<u8> {
    let base = path.rsplit(|&c| c == b'/').next().unwrap_or(path);
    base.strip_suffix(b".rhai").unwrap_or(base).to_vec()
}

#[cfg(feature = "plugins")]
fn load(sh: &mut Shell, name: &[u8], arg: &[u8]) -> ExecResult {
    let Some(path) = plugin_path(sh, arg) else {
        sh.berr(name, "no plugin directory (HOME is not set)");
        return Ok(1);
    };
    // The absolute path, for `savestate`.
    let abs = match (path.first(), &sh.curdir) {
        (Some(b'/'), _) | (_, None) => path.clone(),
        (_, Some(dir)) => crate::builtins::cd::canonicalize(&[dir.as_slice(), b"/", path.as_slice()].concat()),
    };
    let host = sh.plugins.get_or_insert_with(|| std::rc::Rc::new(Host::new())).clone();
    host.load(sh, name, plugin_name(&path), path, abs)
}

#[cfg(not(feature = "plugins"))]
fn load(sh: &mut Shell, name: &[u8], arg: &[u8]) -> ExecResult {
    let _ = (plugin_path, plugin_name);
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

#[cfg(test)]
mod tests {
    use super::plugin_name;

    #[test]
    fn names() {
        assert_eq!(plugin_name(b"/a/b/greet.rhai"), b"greet");
        assert_eq!(plugin_name(b"./x"), b"x");
        assert_eq!(plugin_name(b"greet.rhai"), b"greet");
    }
}
