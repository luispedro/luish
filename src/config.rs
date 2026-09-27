//! `$XDG_CONFIG_HOME/luish/config.toml` (DEVELOPING.md): luish's
//! settings in TOML, read by interactive shells before `rc.d`. Each table
//! under `options` is a group of settings, and each key in it means the
//! same as `setopt -p GROUP KEY=VALUE`, with TOML's types:
//!
//! ```toml
//! [options.history]
//! file = "~/.histfile"
//! share = true
//! ```
//!
//! Options take booleans, numbers take integers and text takes strings,
//! where a leading `~` is expanded (nothing else is). An unknown key, or a
//! value of the wrong type, is reported with its line and skipped; a file
//! that isn't valid TOML is reported and ignored. The rc cache
//! (`startcache.rs`) records the file, so a warm start doesn't read it.

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

/// Reads the file at `path`, if there is one, and applies its settings.
pub fn load(sh: &mut Shell, path: &[u8]) {
    let Ok(bytes) = std::fs::read(crate::interactive::to_path(path)) else {
        return;
    };
    let err = |sh: &Shell, offset: usize, msg: &str| report(sh, path, line_of(&bytes, offset), msg);
    let Ok(text) = std::str::from_utf8(&bytes) else {
        err(sh, 0, "not valid UTF-8");
        return;
    };
    let mut root = match toml_span::parse(text) {
        Ok(root) => root,
        Err(e) => {
            err(sh, e.span.start, &e.to_string());
            return;
        }
    };
    let ValueInner::Table(root) = root.take() else {
        return;
    };
    for (key, value) in in_order(root) {
        match &*key.name {
            "options" => match value.as_table() {
                Some(_) => options(sh, value, &err),
                None => err(sh, value.span.start, "options: not a table"),
            },
            name => err(sh, key.span.start, &format!("unknown key: {name}")),
        }
    }
}

/// The entries of a table in the order of the file (`Table` sorts them).
fn in_order(table: Table<'_>) -> Vec<(toml_span::value::Key<'_>, Value<'_>)> {
    let mut entries: Vec<_> = table.into_iter().collect();
    entries.sort_by_key(|e| e.0.span.start);
    entries
}

/// The `options` table: one table per group.
fn options(sh: &mut Shell, mut value: Value<'_>, err: &dyn Fn(&Shell, usize, &str)) {
    let ValueInner::Table(groups) = value.take() else {
        return;
    };
    for (key, mut value) in in_order(groups) {
        let Some(group) = find_group(key.name.as_bytes()) else {
            err(sh, key.span.start, &format!("no such group: {}", key.name));
            continue;
        };
        let ValueInner::Table(settings) = value.take() else {
            err(sh, value.span.start, &format!("options.{}: not a table", key.name));
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
fn tilde(sh: &Shell, s: &[u8]) -> Vec<u8> {
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
fn line_of(text: &[u8], offset: usize) -> usize {
    1 + text[..offset.min(text.len())].iter().filter(|&&c| c == b'\n').count()
}

fn report(sh: &Shell, path: &[u8], line: usize, msg: &str) {
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
