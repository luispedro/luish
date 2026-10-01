//! `$XDG_CONFIG_HOME/luish/config.toml` (DEVELOPING.md): luish's
//! settings in TOML, read by interactive shells before `rc.d`. Each table
//! under `options` is a group of settings, and each key in it means the
//! same as `setopt -p GROUP KEY=VALUE`, with TOML's types; a value directly
//! under `options` is `setopt NAME=VALUE`:
//!
//! ```toml
//! [options]
//! autosuggest = true
//!
//! [options.history]
//! file = "~/.histfile"
//! share = true
//! ```
//!
//! The `plugins` table is read by `plugins/package.rs`.
//!
//! Options take booleans, numbers take integers and text takes strings,
//! where a leading `~` is expanded (nothing else is). The `alias` table
//! defines aliases, with global and suffix aliases in its `global` and
//! `suffix` tables:
//!
//! ```toml
//! [alias]
//! ll = "ls -l"
//! [alias.global]
//! G = "| grep"
//! [alias.suffix]
//! pdf = "evince"
//! ```
//!
//! The `bindkey` table binds keys (as `bindkey` takes them) to widgets:
//!
//! ```toml
//! [bindkey]
//! Up = "up-line-or-history"
//! "^X^E" = "undo"
//! ```
//!
//! The `env` table sets exported variables, and its `interactive` table
//! those only for interactive shells; `vars` sets shell variables that
//! aren't exported. `path` adds directories to `PATH`, before or after
//! those it has, skipping those it has already (so that a shell started by
//! `pixi shell` keeps its environment's directories first):
//!
//! ```toml
//! [env]
//! EDITOR = "nvim"
//! [env.interactive]
//! LESS = "-R"
//! [vars]
//! WORDCHARS = "*?_-."
//! [path]
//! before = ["~/bin"]
//! after = ["/opt/tools/bin"]
//! ```
//!
//! A login shell that isn't interactive reads only `env` (not
//! `env.interactive`) and `path` (`load_login`).
//!
//! A directory plugin's `plugin.toml` can have `options`, `alias` and
//! `bindkey` tables too (`load_plugin_manifest`).
//!
//! An unknown key, or a value of the wrong type, is reported with its line
//! and skipped; a file that isn't valid TOML is reported and ignored. The
//! rc cache (`startcache.rs`) records the file, so a warm start doesn't
//! read it.

use crate::lexer::AliasKind;
use crate::options::{Kind, Options, Setting, find_group};
use crate::shell::Shell;
use crate::sys;
use toml_span::value::{Table, Value, ValueInner};

/// The path of `config.toml`, whether it exists or not.
pub fn path(sh: &Shell) -> Option<Vec<u8>> {
    let mut p = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    p.extend_from_slice(b"/luish/config.toml");
    Some(p)
}

/// A TOML basic string.
pub fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\x7f' => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Reads the file at `path`, if there is one, and applies its settings.
pub fn load(sh: &mut Shell, path: &[u8]) {
    let mut dirs = Dirs::default();
    read(sh, path, true, |sh, key, value, err| match &*key.name {
        "plugins" => {}
        "env" | "vars" | "path" => environment(sh, &key.name, value, true, &mut dirs, err),
        name => {
            if !shared_table(sh, &key, value, err) {
                err(sh, key.span.start, &format!("unknown key: {name}"));
            }
        }
    });
    dirs.apply(sh);
}

/// For a login shell that isn't interactive: only the `env` table, without
/// `env.interactive`, and `path`.
pub fn load_login(sh: &mut Shell) {
    let Some(path) = path(sh) else {
        return;
    };
    let mut dirs = Dirs::default();
    read(sh, &path, true, |sh, key, value, err| {
        if let name @ ("env" | "path") = &*key.name {
            environment(sh, name, value, false, &mut dirs, err);
        }
    });
    dirs.apply(sh);
}

/// The tables `env`, `vars` and `path` (whose directories are added to
/// `dirs`, so that they apply after the variables, wherever they are).
fn environment(
    sh: &mut Shell,
    name: &str,
    mut value: Value<'_>,
    interactive: bool,
    dirs: &mut Dirs,
    err: &dyn Fn(&Shell, usize, &str),
) {
    if value.as_table().is_none() {
        return err(sh, value.span.start, &format!("{name}: not a table"));
    }
    let ValueInner::Table(entries) = value.take() else {
        return;
    };
    for (key, mut value) in in_order(entries) {
        let result = match name {
            "path" => dirs.add(sh, &key.name, &mut value, err),
            "env" if &*key.name == "interactive" && value.as_table().is_some() => {
                if interactive && let ValueInner::Table(entries) = value.take() {
                    for (key, value) in in_order(entries) {
                        assign(sh, "env.interactive", &key.name, &value, true)
                            .unwrap_or_else(|msg| err(sh, key.span.start, &msg));
                    }
                }
                Ok(())
            }
            _ => assign(sh, name, &key.name, &value, name == "env"),
        };
        result.unwrap_or_else(|msg| err(sh, key.span.start, &msg));
    }
}

/// Sets the variable `name` of `table` from a TOML string (with a leading
/// `~` expanded) or integer.
fn assign(sh: &mut Shell, table: &str, name: &str, value: &Value<'_>, export: bool) -> Result<(), String> {
    let value = match value.as_ref() {
        ValueInner::String(s) => tilde(sh, s.as_bytes()),
        ValueInner::Integer(n) => n.to_string().into_bytes(),
        v => return Err(format!("{table}.{name}: expected a string, found {}", v.type_str())),
    };
    if !crate::lexer::is_valid_name(name.as_bytes()) {
        return Err(format!("{table}: bad variable name: {name:?}"));
    }
    sh.try_set_var(name.as_bytes(), value)?;
    if export {
        sh.vars.entry(name.as_bytes()).exported = true;
    }
    Ok(())
}

/// The directories of the `path` table.
#[derive(Default)]
struct Dirs {
    before: Vec<Vec<u8>>,
    after: Vec<Vec<u8>>,
}

impl Dirs {
    /// Adds the directories of `before` or `after` (`key`), skipping (and
    /// reporting) those that aren't non-empty strings.
    fn add(
        &mut self,
        sh: &Shell,
        key: &str,
        value: &mut Value<'_>,
        err: &dyn Fn(&Shell, usize, &str),
    ) -> Result<(), String> {
        let list = match key {
            "before" => &mut self.before,
            "after" => &mut self.after,
            _ => return Err(format!("path: unknown key: {key}")),
        };
        if !matches!(value.as_ref(), ValueInner::Array(_)) {
            return Err(format!(
                "path.{key}: expected an array, found {}",
                value.as_ref().type_str()
            ));
        }
        let ValueInner::Array(items) = value.take() else {
            return Ok(());
        };
        for item in &items {
            match item.as_ref() {
                ValueInner::String(s) if !s.is_empty() => list.push(tilde(sh, s.as_bytes())),
                ValueInner::String(_) => err(sh, item.span.start, &format!("path.{key}: empty directory")),
                v => err(
                    sh,
                    item.span.start,
                    &format!("path.{key}: expected a string, found {}", v.type_str()),
                ),
            }
        }
        Ok(())
    }

    /// Puts the directories in `PATH`, and exports it. A directory that it
    /// has already stays where it is, so that a shell started from one
    /// whose `PATH` has another directory first (as `pixi shell` or
    /// `nix-shell` do) keeps it first.
    fn apply(self, sh: &mut Shell) {
        if self.before.is_empty() && self.after.is_empty() {
            return;
        }
        let old = sh.get_var(b"PATH").unwrap_or_default();
        let mut dirs: Vec<Vec<u8>> = Vec::new();
        let has = |dirs: &[Vec<u8>], d: &[u8]| dirs.iter().any(|e| e == d);
        let old_dirs: Vec<Vec<u8>> = match &old[..] {
            b"" => Vec::new(),
            p => p.split(|&c| c == b':').map(<[u8]>::to_vec).collect(),
        };
        for d in self.before {
            if !has(&dirs, &d) && !has(&old_dirs, &d) {
                dirs.push(d);
            }
        }
        dirs.extend(old_dirs);
        for d in self.after {
            if !has(&dirs, &d) {
                dirs.push(d);
            }
        }
        let new = dirs.join(&b':');
        if new != old {
            // PATH can't be readonly this early.
            let _ = sh.try_set_var(b"PATH", new);
            sh.vars.entry(b"PATH").exported = true;
        }
    }
}

/// Applies the `options`, `alias` and `bindkey` tables of a plugin's
/// `plugin.toml`, as in `config.toml` (so a plugin can package a set of
/// options, which override the user's). Its other keys are for `plugins/package.rs`, which
/// also reports a file that isn't TOML (so it isn't reported again here).
#[cfg(feature = "plugins")]
pub fn load_plugin_manifest(sh: &mut Shell, path: &[u8]) {
    read(sh, path, false, |sh, key, value, err| {
        shared_table(sh, &key, value, err);
    });
}

/// The tables that `config.toml` and `plugin.toml` share: `options`,
/// `alias` and `bindkey`. False for another key.
fn shared_table(
    sh: &mut Shell,
    key: &toml_span::value::Key<'_>,
    value: Value<'_>,
    err: &dyn Fn(&Shell, usize, &str),
) -> bool {
    match &*key.name {
        "options" => match value.as_table() {
            Some(_) => options(sh, value, err),
            None => err(sh, value.span.start, "options: not a table"),
        },
        "alias" => match value.as_table() {
            Some(_) => aliases(sh, value, err),
            None => err(sh, value.span.start, "alias: not a table"),
        },
        "bindkey" => match value.as_table() {
            Some(_) => bindkeys(sh, value, err),
            None => err(sh, value.span.start, "bindkey: not a table"),
        },
        _ => return false,
    }
    true
}

/// Reads the TOML file at `path`, if there is one, and calls `f` with each
/// top-level key, in the file's order, and a function that reports errors.
/// A file that isn't TOML is reported if `syntax`.
fn read(
    sh: &mut Shell,
    path: &[u8],
    syntax: bool,
    mut f: impl FnMut(&mut Shell, toml_span::value::Key<'_>, Value<'_>, &dyn Fn(&Shell, usize, &str)),
) {
    let Ok(bytes) = std::fs::read(crate::interactive::to_path(path)) else {
        return;
    };
    let err = |sh: &Shell, offset: usize, msg: &str| report(sh, path, line_of(&bytes, offset), msg);
    let Ok(text) = std::str::from_utf8(&bytes) else {
        if syntax {
            err(sh, 0, "not valid UTF-8");
        }
        return;
    };
    let mut root = match toml_span::parse(text) {
        Ok(root) => root,
        Err(e) => {
            if syntax {
                err(sh, e.span.start, &e.to_string());
            }
            return;
        }
    };
    let ValueInner::Table(root) = root.take() else {
        return;
    };
    for (key, value) in in_order(root) {
        f(sh, key, value, &err);
    }
}

/// The entries of a table in the order of the file (`Table` sorts them).
pub fn in_order(table: Table<'_>) -> Vec<(toml_span::value::Key<'_>, Value<'_>)> {
    let mut entries: Vec<_> = table.into_iter().collect();
    entries.sort_by_key(|e| e.0.span.start);
    entries
}

/// The `options` table: one table per group, and settings by their own
/// names.
fn options(sh: &mut Shell, mut value: Value<'_>, err: &dyn Fn(&Shell, usize, &str)) {
    let ValueInner::Table(groups) = value.take() else {
        return;
    };
    for (key, mut value) in in_order(groups) {
        if value.as_table().is_none() {
            if let Err(msg) = set(sh, &key.name, &value) {
                err(sh, key.span.start, &msg);
            }
            continue;
        }
        let Some(group) = find_group(key.name.as_bytes()) else {
            err(sh, key.span.start, &format!("no such group: {}", key.name));
            continue;
        };
        let ValueInner::Table(settings) = value.take() else {
            continue;
        };
        for (key, value) in in_order(settings) {
            let name = format!("{group}.{}", key.name);
            if let Err(msg) = set(sh, &name, &value) {
                err(sh, key.span.start, &msg);
            }
        }
    }
}

/// The `alias` table: a string is a regular alias, and the tables `global`
/// and `suffix` hold global and suffix aliases.
fn aliases(sh: &mut Shell, mut value: Value<'_>, err: &dyn Fn(&Shell, usize, &str)) {
    let ValueInner::Table(entries) = value.take() else {
        return;
    };
    for (key, mut value) in in_order(entries) {
        let kind = match (&*key.name, value.as_ref()) {
            (_, ValueInner::String(_)) => {
                define(sh, "alias", &key.name, &value, AliasKind::Regular)
                    .unwrap_or_else(|msg| err(sh, key.span.start, &msg));
                continue;
            }
            ("global", ValueInner::Table(_)) => AliasKind::Global,
            ("suffix", ValueInner::Table(_)) => AliasKind::Suffix,
            (name, v) => {
                err(
                    sh,
                    key.span.start,
                    &format!("alias.{name}: expected a string, found {}", v.type_str()),
                );
                continue;
            }
        };
        let ValueInner::Table(entries) = value.take() else {
            continue;
        };
        let table = format!("alias.{}", key.name);
        for (key, value) in in_order(entries) {
            define(sh, &table, &key.name, &value, kind).unwrap_or_else(|msg| err(sh, key.span.start, &msg));
        }
    }
}

/// Defines the alias `name` in `table` from a TOML value.
fn define(sh: &mut Shell, table: &str, name: &str, value: &Value<'_>, kind: AliasKind) -> Result<(), String> {
    let ValueInner::String(s) = value.as_ref() else {
        return Err(format!(
            "{table}.{name}: expected a string, found {}",
            value.as_ref().type_str()
        ));
    };
    // What `alias` couldn't define: the name ends at the first `=`, and an
    // argument can't hold a NUL byte (the lexer's mark for a suffix alias).
    if name.is_empty() || name.contains(['=', '\0']) {
        return Err(format!("{table}: bad alias name: {name:?}"));
    }
    let (name, value) = (name.as_bytes().to_vec(), s.as_bytes().to_vec());
    let aliases = std::rc::Rc::make_mut(&mut sh.aliases);
    match kind {
        AliasKind::Suffix => aliases.insert_suffix(name, value),
        k => aliases.insert(name, value, k == AliasKind::Global),
    }
    Ok(())
}

/// The `bindkey` table: each key sequence and the widget it is bound to.
fn bindkeys(sh: &mut Shell, mut value: Value<'_>, err: &dyn Fn(&Shell, usize, &str)) {
    let ValueInner::Table(entries) = value.take() else {
        return;
    };
    for (key, value) in in_order(entries) {
        let result = match value.as_ref() {
            ValueInner::String(w) => crate::interactive::keys::bind_widget(sh, key.name.as_bytes(), w.as_bytes()),
            v => Err(format!("expected a string, found {}", v.type_str())),
        };
        if let Err(msg) = result {
            err(sh, key.span.start, &format!("bindkey.{}: {msg}", key.name));
        }
    }
}

/// Sets the setting `name` from a TOML value.
fn set(sh: &mut Shell, name: &str, value: &Value<'_>) -> Result<(), String> {
    let want = |what: &str| Err(format!("{name}: expected {what}, found {}", value.as_ref().type_str()));
    match (Options::find(name.as_bytes()), value.as_ref()) {
        (None, _) => Err(format!("no such option: {name}")),
        (Some(Setting::Flag(o, sense)), ValueInner::Boolean(on)) => {
            sh.options.set(o, *on == sense);
            Ok(())
        }
        (Some(Setting::Flag(..)), _) => want("a boolean"),
        (Some(Setting::Value(var, Kind::Number)), ValueInner::Integer(n)) if *n >= 0 => {
            sh.try_set_var(var, n.to_string().into_bytes())
        }
        (Some(Setting::Value(_, Kind::Number)), ValueInner::Integer(n)) => Err(format!("{name}: bad value: {n}")),
        (Some(Setting::Value(_, Kind::Number)), _) => want("an integer"),
        (Some(Setting::Value(var, Kind::Text)), ValueInner::String(s)) => sh.try_set_var(var, tilde(sh, s.as_bytes())),
        (Some(Setting::Value(_, Kind::Text)), _) => want("a string"),
    }
}

/// Expands a leading `~` or `~user`, up to the first `/`, as the shell
/// does.
pub fn tilde(sh: &Shell, s: &[u8]) -> Vec<u8> {
    let Some(rest) = s.strip_prefix(b"~") else {
        return s.to_vec();
    };
    let end = rest.iter().position(|&c| c == b'/').unwrap_or(rest.len());
    let (user, rest) = rest.split_at(end);
    let home = if user.is_empty() {
        sh.get_var(b"HOME").or_else(sys::own_home_dir)
    } else {
        sys::home_dir(user)
    };
    match home {
        Some(h) => [&h[..], rest].concat(),
        None => s.to_vec(),
    }
}

/// The line (from 1) of a byte offset.
pub fn line_of(text: &[u8], offset: usize) -> usize {
    1 + text[..offset.min(text.len())].iter().filter(|&&c| c == b'\n').count()
}

/// Prints `luish: PATH: line N: MSG`.
pub fn report(sh: &Shell, path: &[u8], line: usize, msg: &str) {
    let mut s = sh.arg0.clone();
    s.extend_from_slice(b": ");
    s.extend_from_slice(path);
    s.extend_from_slice(format!(": line {line}: {msg}\n").as_bytes());
    sys::write_all(2, &s);
}

#[cfg(test)]
mod tests {
    use super::line_of;

    #[test]
    fn lines() {
        assert_eq!(line_of(b"a\nb\nc", 0), 1);
        assert_eq!(line_of(b"a\nb\nc", 2), 2);
        assert_eq!(line_of(b"a\nb\nc", 4), 3);
        assert_eq!(line_of(b"a\n", 9), 2);
    }
}
