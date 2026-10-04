//! Plugin packages (DEVELOPING.md): the `[plugins]` table of `config.toml`,
//! the manifests of plugins (`plugin.toml`, with their dependencies),
//! resolving plugins with their dependencies, and `plugins.lock`.
//!
//! ```toml
//! [plugins.available]
//! smarty-prompt = { gh = "luispedro/smarty-prompt", branch = "main" }
//!
//! [plugins.enabled]
//! std.bash-completion = "*"      # or "std/bash-completion" = "*"
//! ```
//!
//! A **source** is where plugins are: a git repository (`gh`, `git`) at a
//! branch, a tag, a commit or its `HEAD`, or a local `path`, with an
//! optional `subdir`. A source is one plugin if it holds an entry point
//! (`init.lsh`, `extension.rhai`, ...), and otherwise a collection of
//! plugins (`NAME.rhai`, `NAME.lsh`, `NAME/`) and of sub-collections (its
//! other directories). `plugins.available` names sources, and `std` names
//! luish's own collection. `plugins.enabled` lists the plugins that
//! interactive shells load, as `SOURCE/PATH` (a plugin of a collection,
//! `NAME` or `SUB/.../NAME`, which is also its name once loaded), `NAME` (a
//! named source, else a plugin in the plugin directory) or `NAME = { gh =
//! ... }` (a source of its own). A directory plugin's `plugin.toml` lists its
//! dependencies in the same way, where a plain `NAME` is a plugin of the same
//! collection, and `/PATH` one of the same source.
//!
//! An entry can also be a table with the version requirement and the
//! plugin's options (`NAME = { version = "*", options = { level = 2 } }`),
//! which the plugin declares in its `plugin.toml`, under `[plugin-options]`
//! (a type, and a default or `required = true`). A plugin loaded twice (as
//! the dependency of two plugins, say) must be given the same options.
//!
//! `plugin sync` resolves all of these (fetching with git, `fetch.rs`) and
//! writes `plugins.lock`, which pins each git source to a commit; startup and
//! `plugin load` resolve with the pins, without the network.

use super::ui::{Kind as Paint, Ui};
use super::{Found, Kind, OptValue, PluginOptions, fetch};
pub(super) use crate::config::toml_str;
use crate::config::{in_order, line_of, report, tilde};
use crate::interactive::to_path;
use crate::shell::{ExecResult, Shell};
use toml_span::value::{Table, Value, ValueInner};

/// `std`: the collection in luish's repository.
const STD_REPO: &str = "luispedro/luish";
const STD_SUBDIR: &str = "luish-std-plugins";

/// The ref `std` is taken from: the tag of this version of luish (which the
/// release workflow checks against `Cargo.toml`), so that the plugins are
/// those released with the shell that runs them, not newer ones that may
/// need a newer luish.
fn std_ref() -> GitRef {
    GitRef::Tag(concat!("v", env!("CARGO_PKG_VERSION")).into())
}

/// The version of `plugins.lock`'s layout.
const LOCK_VERSION: i64 = 1;

/// Which commit of a git repository.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GitRef {
    /// The remote's `HEAD` (its default branch).
    Head,
    Branch(String),
    Tag(String),
    /// A full commit hash.
    Rev(String),
}

impl GitRef {
    /// The key and value in the lock and the table, or `None` for `HEAD`.
    fn field(&self) -> Option<(&'static str, &str)> {
        match self {
            GitRef::Head => None,
            GitRef::Branch(b) => Some(("branch", b)),
            GitRef::Tag(t) => Some(("tag", t)),
            GitRef::Rev(r) => Some(("rev", r)),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Origin {
    Git {
        url: String,
        at: GitRef,
    },
    /// A file or directory, used where it is (an absolute path).
    Local(Vec<u8>),
}

/// Where plugins are.
#[derive(Clone, Debug)]
struct Source {
    origin: Origin,
    subdir: Option<String>,
    /// How messages and the lock show it: its name, or its `gh`, `git` or
    /// `path` as written.
    label: String,
}

/// Where an entry was written, for messages.
#[derive(Clone, Debug)]
struct Loc {
    file: Vec<u8>,
    line: usize,
}

/// What an entry of `plugins.enabled` or `dependencies` names.
#[derive(Clone, Debug)]
enum Target {
    /// `NAME = "*"` (`None`) or `SOURCE/PATH = "*"`.
    Named(Option<String>),
    /// `"/PATH" = "*"` in a manifest: a plugin of the same source.
    InSource,
    /// `NAME = { gh = ..., plugin = ... }`.
    Inline(Source, Option<String>),
}

/// An entry of `plugins.enabled` or of a manifest's `dependencies`.
#[derive(Clone, Debug)]
struct Entry {
    /// The plugin's name, or its path in a source: the key less the source
    /// (`SOURCE/PATH`) or the leading `/`.
    name: String,
    target: Target,
    /// The options it gives the plugin.
    options: Vec<(String, Given)>,
    loc: Option<Loc>,
}

/// The value of an option given to a plugin: in a TOML file, with its type,
/// or as `OPTION=VALUE` to `plugin load` (or `plugin restore`), as text.
#[derive(Clone, Debug)]
enum Given {
    Toml(OptValue),
    Text(String),
}

/// The type of a plugin's option.
#[derive(Clone, Copy, PartialEq, Debug)]
enum OptType {
    Str,
    Int,
    Bool,
}

impl OptType {
    fn of(v: &OptValue) -> OptType {
        match v {
            OptValue::Str(_) => OptType::Str,
            OptValue::Int(_) => OptType::Int,
            OptValue::Bool(_) => OptType::Bool,
        }
    }

    fn what(self) -> &'static str {
        match self {
            OptType::Str => "a string",
            OptType::Int => "an integer",
            OptType::Bool => "a boolean",
        }
    }
}

/// An option that a plugin declares in the `[plugin-options]` of its
/// `plugin.toml`.
#[derive(Clone, Debug)]
struct Decl {
    name: String,
    ty: OptType,
    default: Option<OptValue>,
    required: bool,
}

/// An option's value as TOML writes it, for messages.
fn shown(v: &OptValue) -> String {
    match v {
        OptValue::Str(s) => toml_str(s),
        v => v.text(),
    }
}

/// The options of the plugin `name`, which declares `decls`, given `given`:
/// those given, converted to their types, and the defaults of the others.
fn apply(name: &str, decls: &[Decl], given: &[(String, Given)]) -> Result<PluginOptions, String> {
    for (i, (k, _)) in given.iter().enumerate() {
        if !decls.iter().any(|d| d.name == *k) {
            return Err(match decls.is_empty() {
                true => format!("{name}: takes no options"),
                false => {
                    let names: Vec<_> = decls.iter().map(|d| d.name.as_str()).collect();
                    format!("{name}: no option {k} (it has {})", names.join(", "))
                }
            });
        }
        if given[..i].iter().any(|(k2, _)| k2 == k) {
            return Err(format!("{name}: option {k} given twice"));
        }
    }
    let mut out = Vec::new();
    for d in decls {
        let value = match given.iter().find(|(k, _)| *k == d.name).map(|(_, v)| v) {
            Some(Given::Toml(v)) if OptType::of(v) == d.ty => v.clone(),
            Some(Given::Toml(v)) => {
                let (want, found) = (d.ty.what(), OptType::of(v).what());
                return Err(format!("{name}: option {}: expected {want}, found {found}", d.name));
            }
            Some(Given::Text(t)) => match (d.ty, t.as_str()) {
                (OptType::Str, _) => OptValue::Str(t.clone()),
                (OptType::Bool, "true") => OptValue::Bool(true),
                (OptType::Bool, "false") => OptValue::Bool(false),
                (OptType::Int, _) if t.parse::<i64>().is_ok() => OptValue::Int(t.parse().unwrap_or_default()),
                (ty, _) => {
                    let want = match ty {
                        OptType::Bool => "true or false",
                        _ => ty.what(),
                    };
                    return Err(format!("{name}: option {}: expected {want}, found {t:?}", d.name));
                }
            },
            None => match &d.default {
                Some(v) => v.clone(),
                None if d.required => return Err(format!("{name}: option {} is required", d.name)),
                None => continue,
            },
        };
        out.push((d.name.clone(), value));
    }
    Ok(out)
}

/// Where the options `a` and `b` of a plugin that declares `decls` differ,
/// as `OPTION = VALUE, ...` for each.
fn differences(decls: &[Decl], a: &PluginOptions, b: &PluginOptions) -> (String, String) {
    let get = |o: &PluginOptions, k: &str| o.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let side = |v: Option<OptValue>, k: &str| match v {
        Some(v) => format!("{k} = {}", shown(&v)),
        None => format!("{k} unset"),
    };
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for d in decls {
        let (va, vb) = (get(a, &d.name), get(b, &d.name));
        if va != vb {
            x.push(side(va, &d.name));
            y.push(side(vb, &d.name));
        }
    }
    (x.join(", "), y.join(", "))
}

/// The `[plugins]` table.
#[derive(Default)]
struct Config {
    available: Vec<(String, Source)>,
    enabled: Vec<Entry>,
}

impl Entry {
    /// The entry's key, with `/` for TOML's dots.
    fn key(&self) -> String {
        match &self.target {
            Target::Named(Some(src)) => format!("{src}/{}", self.name),
            Target::InSource => format!("/{}", self.name),
            _ => self.name.clone(),
        }
    }
}

impl Config {
    /// The source called `name`: in `plugins.available`, or `std`.
    fn named(&self, name: &str) -> Option<Source> {
        if let Some((_, s)) = self.available.iter().find(|(n, _)| n == name) {
            return Some(s.clone());
        }
        (name == "std").then(|| Source {
            origin: Origin::Git {
                url: github_url(STD_REPO),
                at: std_ref(),
            },
            subdir: Some(STD_SUBDIR.into()),
            label: "std".into(),
        })
    }
}

/// An error, with where it was written if it comes from a file.
struct Problem {
    loc: Option<Loc>,
    msg: String,
}

/// Prints problems: those with a place as `luish: FILE: line N: MSG`, the
/// others as errors of the command `cmd` (or of the shell, at startup).
fn print_problems(sh: &Shell, cmd: Option<&[u8]>, problems: &[Problem]) {
    for p in problems {
        match (&p.loc, cmd) {
            (Some(l), _) => report(sh, &l.file, l.line, &p.msg),
            (None, Some(cmd)) => sh.berr(cmd, &p.msg),
            (None, None) => sh.error(&p.msg),
        }
    }
}

pub(super) fn github_url(repo: &str) -> String {
    format!("https://github.com/{repo}.git")
}

/// A plugin's or a source's name: not empty, without `/`, not starting
/// with `.`.
pub(super) fn valid_name(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('.') && !s.contains(['/', '\0', '\n'])
}

/// A plugin's path in a collection: `NAME`, or `SUB/.../NAME` in its
/// sub-collections.
pub(super) fn valid_path(s: &str) -> bool {
    s.split('/').all(valid_name)
}

// ----------------------------------------------------------------------
// Reading the table and manifests

/// Reads a TOML file, for [`Reader`].
struct Reader<'a> {
    sh: &'a Shell,
    file: &'a [u8],
    text: &'a str,
    /// The directory that relative paths are relative to.
    base: &'a [u8],
    problems: &'a mut Vec<Problem>,
}

impl Reader<'_> {
    fn loc(&self, offset: usize) -> Loc {
        Loc {
            file: self.file.to_vec(),
            line: line_of(self.text.as_bytes(), offset),
        }
    }

    fn err(&mut self, offset: usize, msg: impl Into<String>) {
        let loc = Some(self.loc(offset));
        self.problems.push(Problem { loc, msg: msg.into() });
    }

    /// The `[plugins]` table.
    fn plugins(&mut self, mut value: Value<'_>) -> Config {
        let mut config = Config::default();
        let ValueInner::Table(t) = value.take() else {
            self.err(value.span.start, "plugins: not a table");
            return config;
        };
        for (key, mut value) in in_order(t) {
            match &*key.name {
                "available" => {
                    let ValueInner::Table(t) = value.take() else {
                        self.err(value.span.start, "plugins.available: not a table");
                        continue;
                    };
                    for (key, value) in in_order(t) {
                        if !valid_name(&key.name) {
                            self.err(key.span.start, format!("{}: bad source name", key.name));
                        } else if let Some((source, _)) = self.source(&key.name, &value, false) {
                            config.available.push((key.name.to_string(), source));
                        }
                    }
                }
                "enabled" => self.entries(value, "plugins.enabled", &mut config.enabled),
                name => self.err(key.span.start, format!("unknown key: plugins.{name}")),
            }
        }
        config
    }

    /// A table of entries: `plugins.enabled`, or a manifest's
    /// `dependencies`.
    fn entries(&mut self, mut value: Value<'_>, what: &str, out: &mut Vec<Entry>) {
        let ValueInner::Table(t) = value.take() else {
            self.err(value.span.start, format!("{what}: not a table"));
            return;
        };
        for (key, value) in in_order(t) {
            let start = key.span.start;
            let key = &*key.name;
            match value.as_ref() {
                ValueInner::String(req) => {
                    if self.requirement(start, key, req) {
                        self.named_entry(start, key, Vec::new(), out);
                    }
                }
                ValueInner::Table(t) if is_source(t) => {
                    let Some(options) = self.entry_table(key, t, true) else {
                        continue;
                    };
                    if !valid_name(key) {
                        self.err(start, format!("{key}: bad plugin name"));
                    } else if let Some((source, plugin)) = self.source(key, &value, true) {
                        out.push(Entry {
                            name: key.into(),
                            target: Target::Inline(source, plugin),
                            options,
                            loc: Some(self.loc(start)),
                        });
                    }
                }
                ValueInner::Table(t) if is_entry(t) => {
                    if let Some(options) = self.entry_table(key, t, false) {
                        self.named_entry(start, key, options, out);
                    }
                }
                // `SOURCE.PATH = "*"`, which TOML reads as tables.
                ValueInner::Table(t) => {
                    if !valid_name(key) {
                        self.err(start, format!("{key}: bad source name"));
                        continue;
                    }
                    self.nested(key, "", key, t, out);
                }
                other => self.err(
                    start,
                    format!("{key}: expected \"*\" or a table, found {}", other.type_str()),
                ),
            }
        }
    }

    /// The entry `key = "*"` (or a table with `version` or `options`): `NAME`,
    /// `SOURCE/PATH` or `/PATH`.
    fn named_entry(&mut self, start: usize, key: &str, options: Vec<(String, Given)>, out: &mut Vec<Entry>) {
        let (target, name) = match (key.strip_prefix('/'), key.split_once('/')) {
            (Some(path), _) => (Target::InSource, path),
            (None, Some((s, n))) if valid_name(s) => (Target::Named(Some(s.into())), n),
            (None, Some(_)) => (Target::Named(None), ""),
            (None, None) => (Target::Named(None), key),
        };
        if !valid_path(name) {
            self.err(start, format!("{key}: bad plugin name (NAME or SOURCE/PATH)"));
            return;
        }
        out.push(Entry {
            name: name.into(),
            target,
            options,
            loc: Some(self.loc(start)),
        });
    }

    /// The `version` and `options` of an entry's table `t`: its options, or
    /// `None` if they are wrong. Other keys are an error, but for a `source`
    /// (whose keys [`Reader::source`] reads).
    fn entry_table(&mut self, key: &str, t: &Table<'_>, source: bool) -> Option<Vec<(String, Given)>> {
        let mut options = Vec::new();
        let mut ok = true;
        let mut fields: Vec<_> = t.iter().collect();
        fields.sort_by_key(|(k, _)| k.span.start);
        for (k, v) in fields {
            match (&*k.name, v.as_ref()) {
                ("version", ValueInner::String(req)) => ok &= self.requirement(k.span.start, key, req),
                ("version", _) => {
                    self.err(k.span.start, format!("{key}.version: expected a string"));
                    ok = false;
                }
                ("options", ValueInner::Table(o)) => {
                    let mut o: Vec<_> = o.iter().collect();
                    o.sort_by_key(|(k, _)| k.span.start);
                    for (name, value) in o {
                        let value = match value.as_ref() {
                            ValueInner::String(s) => OptValue::Str(s.to_string()),
                            ValueInner::Integer(n) => OptValue::Int(*n),
                            ValueInner::Boolean(b) => OptValue::Bool(*b),
                            _ => {
                                let msg = format!(
                                    "{key}.options.{}: expected a string, an integer or a boolean",
                                    name.name
                                );
                                self.err(name.span.start, msg);
                                ok = false;
                                continue;
                            }
                        };
                        options.push((name.name.to_string(), Given::Toml(value)));
                    }
                }
                ("options", _) => {
                    self.err(k.span.start, format!("{key}.options: not a table"));
                    ok = false;
                }
                _ if source => {}
                (other, _) => {
                    self.err(k.span.start, format!("{key}: unknown key: {other}"));
                    ok = false;
                }
            }
        }
        ok.then_some(options)
    }

    /// The options that a manifest declares in `[plugin-options]`.
    fn declarations(&mut self, mut value: Value<'_>) -> Vec<Decl> {
        let mut out = Vec::new();
        let ValueInner::Table(t) = value.take() else {
            self.err(value.span.start, "plugin-options: not a table");
            return out;
        };
        for (key, value) in in_order(t) {
            let (start, name) = (key.span.start, &*key.name);
            if !super::valid_option_name(name) {
                self.err(
                    start,
                    format!("plugin-options.{name}: bad option name (letters, digits, - and _)"),
                );
                continue;
            }
            let Some(t) = value.as_table() else {
                self.err(
                    start,
                    format!("plugin-options.{name}: expected a table ({{ type = \"str\" }})"),
                );
                continue;
            };
            let mut fields: Vec<_> = t.iter().collect();
            fields.sort_by_key(|(k, _)| k.span.start);
            let (mut ty, mut default, mut required, mut ok) = (None, None, false, true);
            for (k, v) in fields {
                let mut bad = |r: &mut Self, msg: &str| {
                    r.err(k.span.start, format!("plugin-options.{name}.{}: {msg}", k.name));
                    ok = false;
                };
                match (&*k.name, v.as_ref()) {
                    ("type", ValueInner::String(t)) => match &**t {
                        "str" => ty = Some(OptType::Str),
                        "int" => ty = Some(OptType::Int),
                        "boolean" => ty = Some(OptType::Bool),
                        _ => bad(self, "expected \"str\", \"int\" or \"boolean\""),
                    },
                    ("default", ValueInner::String(s)) => default = Some(OptValue::Str(s.to_string())),
                    ("default", ValueInner::Integer(n)) => default = Some(OptValue::Int(*n)),
                    ("default", ValueInner::Boolean(b)) => default = Some(OptValue::Bool(*b)),
                    ("required", ValueInner::Boolean(b)) => required = *b,
                    ("type", _) => bad(self, "expected \"str\", \"int\" or \"boolean\""),
                    ("default", _) => bad(self, "expected a string, an integer or a boolean"),
                    ("required", _) => bad(self, "not a boolean"),
                    _ => bad(self, "unknown key"),
                }
            }
            let Some(ty) = ty else {
                if ok {
                    self.err(start, format!("plugin-options.{name}: no type"));
                }
                continue;
            };
            if let Some(d) = default.as_ref().filter(|d| OptType::of(d) != ty) {
                let msg = format!(
                    "plugin-options.{name}: the default is {}, not {}",
                    OptType::of(d).what(),
                    ty.what()
                );
                self.err(start, msg);
                continue;
            }
            if required && default.is_some() {
                self.err(start, format!("plugin-options.{name}: both required and a default"));
                continue;
            }
            if ok {
                out.push(Decl {
                    name: name.to_string(),
                    ty,
                    default,
                    required,
                });
            }
        }
        out
    }

    /// The entries of `SOURCE.PATH = "*"`: those of the table `t`, at `path`
    /// in the source `src` (`shown` as written).
    fn nested(&mut self, src: &str, path: &str, shown: &str, t: &Table<'_>, out: &mut Vec<Entry>) {
        let mut plugins: Vec<_> = t.iter().collect();
        plugins.sort_by_key(|(k, _)| k.span.start);
        for (k, v) in plugins {
            let name = &*k.name;
            let full = format!("{shown}.{name}");
            if !valid_name(name) {
                self.err(k.span.start, format!("{full}: bad plugin name"));
                continue;
            }
            let path = match path {
                "" => name.to_string(),
                _ => format!("{path}/{name}"),
            };
            match v.as_ref() {
                ValueInner::String(req) if self.requirement(k.span.start, &full, req) => out.push(Entry {
                    name: path,
                    target: Target::Named(Some(src.into())),
                    options: Vec::new(),
                    loc: Some(self.loc(k.span.start)),
                }),
                ValueInner::String(_) => {}
                ValueInner::Table(t) if is_entry(t) => {
                    if let Some(options) = self.entry_table(&full, t, false) {
                        out.push(Entry {
                            name: path,
                            target: Target::Named(Some(src.into())),
                            options,
                            loc: Some(self.loc(k.span.start)),
                        });
                    }
                }
                ValueInner::Table(t) => self.nested(src, &path, &full, t, out),
                _ => self.err(k.span.start, format!("{full}: expected \"*\"")),
            }
        }
    }

    /// Checks a version requirement: only `"*"` for now.
    fn requirement(&mut self, offset: usize, key: &str, req: &str) -> bool {
        if req == "*" {
            return true;
        }
        self.err(
            offset,
            format!("{key}: unsupported version requirement {req:?} (only \"*\")"),
        );
        false
    }

    /// A source's table. With `plugin`, it can say which plugin of a
    /// collection it means.
    fn source(&mut self, name: &str, value: &Value<'_>, plugin: bool) -> Option<(Source, Option<String>)> {
        let Some(t) = value.as_table() else {
            self.err(
                value.span.start,
                format!("{name}: expected a table ({{ gh = \"OWNER/REPO\" }})"),
            );
            return None;
        };
        let mut fields: Vec<(&str, String, usize)> = Vec::new();
        let mut ok = true;
        let mut entries: Vec<_> = t.iter().collect();
        entries.sort_by_key(|(k, _)| k.span.start);
        for (k, v) in entries {
            // (Read by `entry_table`.)
            if plugin && ["version", "options"].contains(&&*k.name) {
                continue;
            }
            let known = ["gh", "git", "path", "branch", "tag", "rev", "subdir"].contains(&&*k.name)
                || (plugin && k.name == "plugin");
            match v.as_str() {
                _ if !known => {
                    self.err(k.span.start, format!("{name}: unknown key: {}", k.name));
                    ok = false;
                }
                Some(s) => fields.push((
                    ["gh", "git", "path", "branch", "tag", "rev", "subdir", "plugin"]
                        .into_iter()
                        .find(|f| *f == k.name)
                        .unwrap_or(""),
                    s.to_string(),
                    k.span.start,
                )),
                None => {
                    self.err(k.span.start, format!("{name}.{}: expected a string", k.name));
                    ok = false;
                }
            }
        }
        let get = |f: &str| fields.iter().find(|(k, _, _)| *k == f).map(|(_, v, o)| (v.clone(), *o));
        let at = |sel: &str| get(sel);
        let start = value.span.start;
        let whence: Vec<_> = ["gh", "git", "path"].into_iter().filter(|f| get(f).is_some()).collect();
        let refs: Vec<_> = ["branch", "tag", "rev"]
            .into_iter()
            .filter(|f| get(f).is_some())
            .collect();
        if whence.len() != 1 {
            self.err(start, format!("{name}: needs one of gh, git and path"));
            return None;
        }
        if refs.len() > 1 {
            self.err(start, format!("{name}: at most one of branch, tag and rev"));
            return None;
        }
        let gitref = match refs.first().and_then(|f| at(f).map(|v| (*f, v))) {
            None => GitRef::Head,
            Some(("rev", (r, o))) => {
                let r = r.to_ascii_lowercase();
                if !fetch::is_hash(&r) {
                    self.err(o, format!("{name}.rev: not a full commit hash"));
                    return None;
                }
                GitRef::Rev(r)
            }
            Some((f, (v, o))) if v.is_empty() || v.starts_with('-') || v.contains(['\0', '\n']) => {
                self.err(o, format!("{name}.{f}: bad value {v:?}"));
                return None;
            }
            Some(("branch", (b, _))) => GitRef::Branch(b),
            Some((_, (t, _))) => GitRef::Tag(t),
        };
        let subdir = match get("subdir") {
            None => None,
            Some((s, o)) => {
                let s = s.trim_end_matches('/');
                let bad = s.starts_with('/') || s.split('/').any(|c| c == ".." || c.is_empty()) || s.contains('\0');
                if bad && !s.is_empty() {
                    self.err(o, format!("{name}.subdir: must be a relative path inside the source"));
                    return None;
                }
                (!s.is_empty()).then(|| s.to_string())
            }
        };
        let (origin, label) = match whence[0] {
            "gh" => {
                let (repo, o) = get("gh").unwrap_or_default();
                let valid = repo.split_once('/').is_some_and(|(owner, r)| {
                    let ok = |s: &str| {
                        !s.is_empty()
                            && !s.starts_with(['.', '-'])
                            && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
                    };
                    ok(owner) && ok(r)
                });
                if !valid {
                    self.err(o, format!("{name}.gh: expected OWNER/REPO, found {repo:?}"));
                    return None;
                }
                let url = github_url(&repo);
                (Origin::Git { url, at: gitref }, repo)
            }
            "git" => {
                let (url, o) = get("git").unwrap_or_default();
                if url.is_empty() || url.starts_with('-') || url.contains(['\0', '\n']) {
                    self.err(o, format!("{name}.git: bad URL {url:?}"));
                    return None;
                }
                (
                    Origin::Git {
                        url: url.clone(),
                        at: gitref,
                    },
                    url,
                )
            }
            _ => {
                let (path, o) = get("path").unwrap_or_default();
                if !refs.is_empty() {
                    self.err(o, format!("{name}: branch, tag and rev need gh or git"));
                    return None;
                }
                let mut p = tilde(self.sh, path.as_bytes());
                if p.first() != Some(&b'/') {
                    p = [self.base, b"/", &p].concat();
                }
                let p = crate::builtins::cd::canonicalize(&p);
                (Origin::Local(p), path)
            }
        };
        if !ok {
            return None;
        }
        let label = if plugin { label } else { name.to_string() };
        let which = get("plugin").map(|(p, _)| p);
        if let Some(p) = which.as_ref().filter(|p| !valid_path(p)) {
            self.err(start, format!("{name}.plugin: bad plugin name {p:?}"));
            return None;
        }
        Some((Source { origin, subdir, label }, which))
    }
}

/// Whether a table in `plugins.enabled` or `dependencies` is a source (and
/// not `SOURCE.PATH = "*"`).
fn is_source(t: &Table<'_>) -> bool {
    t.keys().any(|k| ["gh", "git", "path"].contains(&&*k.name))
}

/// Whether a table in `plugins.enabled` or `dependencies` is an entry with
/// a version or options, or empty (the same as `"*"`), and not
/// `SOURCE.PATH = "*"`.
fn is_entry(t: &Table<'_>) -> bool {
    t.is_empty() || t.keys().any(|k| ["version", "options"].contains(&&*k.name))
}

/// Parses the TOML file `file` and gives its top-level table to `f`.
/// `Ok(None)` if the file doesn't exist. Messages call it `shown`.
fn with_toml<R>(file: &[u8], shown: &[u8], f: impl FnOnce(&str, Table<'_>) -> R) -> Result<Option<R>, Problem> {
    match std::fs::read(to_path(file)) {
        Ok(bytes) => with_toml_text(&bytes, shown, f),
        Err(_) => Ok(None),
    }
}

/// [`with_toml`] on the text of a file.
fn with_toml_text<R>(bytes: &[u8], shown: &[u8], f: impl FnOnce(&str, Table<'_>) -> R) -> Result<Option<R>, Problem> {
    let problem = |offset: usize, msg: String| Problem {
        loc: Some(Loc {
            file: shown.to_vec(),
            line: line_of(bytes, offset),
        }),
        msg,
    };
    let text = std::str::from_utf8(bytes).map_err(|_| problem(0, "not valid UTF-8".into()))?;
    let mut root = toml_span::parse(text).map_err(|e| problem(e.span.start, e.to_string()))?;
    match root.take() {
        ValueInner::Table(t) => Ok(Some(f(text, t))),
        _ => Ok(None),
    }
}

fn parent(path: &[u8]) -> &[u8] {
    &path[..path
        .iter()
        .rposition(|&c| c == b'/')
        .unwrap_or(0)
        .max(1)
        .min(path.len())]
}

/// Reads the `[plugins]` table of `config.toml`: an empty table if there is
/// none, or `Err` if the file isn't TOML. Errors in the table go to
/// `problems`.
fn read_config(sh: &Shell, problems: &mut Vec<Problem>) -> Result<Config, Problem> {
    let Some(file) = crate::config::path(sh) else {
        return Ok(Config::default());
    };
    match std::fs::read(to_path(&file)) {
        Ok(bytes) => parse_config(sh, &file, &bytes, problems),
        Err(_) => Ok(Config::default()),
    }
}

/// The `[plugins]` table of the text `bytes` of the configuration file
/// `file`, as [`read_config`].
fn parse_config(sh: &Shell, file: &[u8], bytes: &[u8], problems: &mut Vec<Problem>) -> Result<Config, Problem> {
    let config = with_toml_text(bytes, file, |text, mut root| {
        let Some(value) = root.remove("plugins") else {
            return Config::default();
        };
        let mut r = Reader {
            sh,
            file,
            text,
            base: parent(file),
            problems,
        };
        r.plugins(value)
    })?;
    Ok(config.unwrap_or_default())
}

/// The dependencies in `plugin.toml`, the manifest of the directory plugin
/// `dir`, which messages call `shown`, and the options it declares.
/// `library` must be a boolean (it is read by `is_library`); the manifest's
/// other keys (such as `description`) are ignored.
fn manifest(sh: &Shell, dir: &[u8], shown: &[u8], problems: &mut Vec<Problem>) -> (Vec<Entry>, Vec<Decl>) {
    let file = [dir, b"/plugin.toml"].concat();
    let r = with_toml(&file, shown, |text, mut root| {
        let mut out = Vec::new();
        let mut decls = Vec::new();
        let mut r = Reader {
            sh,
            file: shown,
            text,
            base: dir,
            problems,
        };
        if let Some(value) = root.remove("library")
            && !matches!(value.as_ref(), ValueInner::Boolean(_))
        {
            r.err(value.span.start, "library: not a boolean");
        }
        if let Some(value) = root.remove("dependencies") {
            r.entries(value, "dependencies", &mut out);
        }
        if let Some(value) = root.remove("plugin-options") {
            decls = r.declarations(value);
        }
        (out, decls)
    });
    match r {
        Ok(entries) => entries.unwrap_or_default(),
        Err(p) => {
            problems.push(p);
            Default::default()
        }
    }
}

// ----------------------------------------------------------------------
// plugins.lock

/// A git source pinned to a commit.
#[derive(Clone, PartialEq, Debug)]
struct Pin {
    url: String,
    at: GitRef,
    commit: String,
}

fn lock_path(sh: &Shell) -> Option<Vec<u8>> {
    let mut p = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config")?;
    p.extend_from_slice(b"/luish/plugins.lock");
    Some(p)
}

/// Why the lock can't be used. With `newer`, it must not be rewritten.
struct LockError {
    newer: bool,
    msg: String,
}

/// The pins in `plugins.lock` (none if it doesn't exist).
fn read_lock(file: &[u8]) -> Result<Vec<Pin>, LockError> {
    let bad = |msg: String| LockError { newer: false, msg };
    let r = with_toml(file, file, |text, mut root| {
        let at = |offset: usize| line_of(text.as_bytes(), offset);
        match root.get("version").map(|v| (v.as_integer(), v.span.start)) {
            Some((Some(LOCK_VERSION), _)) => {}
            Some((Some(n), _)) if n > LOCK_VERSION => {
                return Err(LockError {
                    newer: true,
                    msg: format!("written by a newer luish (version {n})"),
                });
            }
            Some((_, o)) => return Err(bad(format!("line {}: unknown version", at(o)))),
            None => return Err(bad("no version".into())),
        }
        let mut pins = Vec::new();
        let Some(mut sources) = root.remove("source") else {
            return Ok(pins);
        };
        let ValueInner::Array(sources) = sources.take() else {
            return Err(bad(format!("line {}: source: not an array", at(sources.span.start))));
        };
        for s in sources {
            let field = |k: &str| {
                s.as_table()
                    .and_then(|t| t.get(k))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            };
            let gitref = match (field("branch"), field("tag"), field("rev")) {
                (None, None, None) => Some(GitRef::Head),
                (Some(b), None, None) => Some(GitRef::Branch(b)),
                (None, Some(t), None) => Some(GitRef::Tag(t)),
                (None, None, Some(r)) => Some(GitRef::Rev(r)),
                _ => None,
            };
            match (field("url"), gitref, field("commit")) {
                (Some(url), Some(at), Some(commit)) if fetch::is_hash(&commit) => pins.push(Pin { url, at, commit }),
                _ => return Err(bad(format!("line {}: bad source", at(s.span.start)))),
            }
        }
        Ok(pins)
    });
    match r {
        Ok(Some(r)) => r,
        Ok(None) => Ok(Vec::new()),
        Err(p) => Err(bad(match p.loc {
            Some(l) => format!("line {}: {}", l.line, p.msg),
            None => p.msg,
        })),
    }
}

/// The text of `plugins.lock`: the pins sorted by URL, and the plugins that
/// interactive shells load, sorted by name.
fn lock_text(pins: &[Pin], plugins: &[Resolved]) -> String {
    let mut out = String::from("# Written by luish (plugin sync, plugin update). Don't edit.\n");
    out.push_str(&format!("version = {LOCK_VERSION}\n"));
    let mut pins: Vec<_> = pins.iter().collect();
    pins.sort_by(|a, b| (&a.url, a.at.field()).cmp(&(&b.url, b.at.field())));
    for p in pins {
        out.push_str(&format!("\n[[source]]\nurl = {}\n", toml_str(&p.url)));
        if let Some((k, v)) = p.at.field() {
            out.push_str(&format!("{k} = {}\n", toml_str(v)));
        }
        out.push_str(&format!("commit = {}\n", toml_str(&p.commit)));
    }
    let mut plugins: Vec<_> = plugins.iter().collect();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    for p in plugins {
        let deps: Vec<_> = p.deps.iter().map(|d| toml_str(d)).collect();
        out.push_str(&format!(
            "\n[[plugin]]\nname = {}\nsource = {}\npath = {}\ndependencies = [{}]\n",
            toml_str(&p.name),
            toml_str(&p.source),
            toml_str(&p.rel),
            deps.join(", "),
        ));
    }
    out
}

/// Writes `text` to `file` through a rename, unless it already holds it
/// (so that the startup cache, which records the file, stays valid).
pub(super) fn write_file(file: &[u8], text: &str) -> Result<(), String> {
    if std::fs::read(to_path(file)).is_ok_and(|t| t == text.as_bytes()) {
        return Ok(());
    }
    let tmp = [file, format!(".{}", crate::sys::getpid()).as_bytes()].concat();
    let r = std::fs::create_dir_all(to_path(parent(file)))
        .and_then(|_| std::fs::write(to_path(&tmp), text))
        .and_then(|_| std::fs::rename(to_path(&tmp), to_path(file)));
    if r.is_err() {
        let _ = std::fs::remove_file(to_path(&tmp));
    }
    r.map_err(|e| format!("cannot write {}: {e}", String::from_utf8_lossy(file)))
}

// ----------------------------------------------------------------------
// Resolving

/// A plugin to load, with its dependencies loaded before it.
struct Resolved {
    name: String,
    /// The plugin, as `plugin load` was given it, or absolute.
    found: Found,
    /// Its absolute path, which identifies it.
    abs: Vec<u8>,
    /// Its source's label.
    source: String,
    /// Where it is in its source (`.` for the whole source).
    rel: String,
    deps: Vec<String>,
    /// The options it declares, and those it is given.
    decls: Vec<Decl>,
    options: PluginOptions,
    /// Who gave it the options, for messages: `for NAME` (a plugin that
    /// depends on it), `in plugins.enabled` or `in plugin load`.
    by: String,
}

/// Where a plain `NAME` in a manifest's `dependencies` is looked for.
#[derive(Clone)]
enum Scope {
    /// In `plugins.enabled` and for `plugin load NAME`: a named source, else
    /// the plugin directory.
    Config,
    /// In the collection of the plugin.
    Collection(Coll),
    /// Nowhere: the plugin isn't in a collection.
    None,
}

/// The collection a plugin is in, for its dependencies.
#[derive(Clone)]
struct Coll {
    /// The collection's directory.
    dir: Vec<u8>,
    /// The files of its source (`/PATH` is from there), and the source's
    /// label.
    root: Vec<u8>,
    label: String,
    /// The source's name, if its plugins are named `SOURCE/PATH`; otherwise
    /// (the plugin directory, a path, a source of an entry's own) they are
    /// named after their files.
    src: Option<String>,
}

impl Coll {
    /// The collection of the plugin `found` of the source `label`, whose
    /// files are in `root`.
    fn of(found: &Found, root: &[u8], label: &str, src: Option<&str>) -> Coll {
        Coll {
            dir: parent(&found.path).to_vec(),
            root: root.to_vec(),
            label: label.to_string(),
            src: src.map(Into::into),
        }
    }

    /// The path from the top of the source of the plugin `name` of this
    /// collection.
    fn path_of(&self, name: &str) -> String {
        match self.dir.strip_prefix(&self.root[..]).and_then(|r| r.strip_prefix(b"/")) {
            Some(r) if !r.is_empty() => format!("{}/{name}", String::from_utf8_lossy(r)),
            _ => name.to_string(),
        }
    }

    /// The name of the plugin at `path` from the top of the source.
    fn name_of(&self, path: &str) -> String {
        match &self.src {
            Some(src) => format!("{src}/{path}"),
            None => path.rsplit('/').next().unwrap_or(path).to_string(),
        }
    }
}

/// Whether and how git sources are fetched.
#[derive(PartialEq)]
enum Fetching {
    /// Never: startup, `plugin load`.
    No,
    /// Those that aren't locked or not in the data directory (`plugin sync`).
    Missing,
    /// Also the newest commits of the named sources, or of all if none are
    /// named (`plugin update`).
    Update(Vec<String>),
}

struct Resolver<'a> {
    sh: &'a mut Shell,
    config: &'a Config,
    data: Option<Vec<u8>>,
    locked: Vec<Pin>,
    fetching: Fetching,
    /// The pins used, with their sources' labels.
    used: Vec<(Pin, String)>,
    /// The names given to `plugin update` that named a source.
    updated: Vec<String>,
    /// The plugins resolved, in the order to load them.
    done: Vec<Resolved>,
    /// The plugins being resolved (for cycles and names).
    stack: Vec<(String, Vec<u8>)>,
    problems: Vec<Problem>,
    /// The sources that aren't installed (with `Fetching::No`).
    missing: Vec<String>,
    /// The manifests of local plugins read or looked for, for the startup
    /// cache.
    manifests: Vec<Vec<u8>>,
    interrupted: bool,
    /// Whether to say what is fetched (`plugin sync` and `plugin update`
    /// without `-q`).
    verbose: bool,
    /// Whether to check the plugins' options (not for the plugins that
    /// `plugin sync` installs without their being enabled).
    check_options: bool,
    /// The colours for what it says.
    ui: Ui,
}

impl<'a> Resolver<'a> {
    fn new(sh: &'a mut Shell, config: &'a Config, locked: Vec<Pin>, fetching: Fetching) -> Resolver<'a> {
        let data = fetch::data_dir(sh);
        Resolver {
            sh,
            config,
            data,
            locked,
            fetching,
            used: Vec::new(),
            updated: Vec::new(),
            done: Vec::new(),
            stack: Vec::new(),
            problems: Vec::new(),
            missing: Vec::new(),
            manifests: Vec::new(),
            interrupted: false,
            verbose: false,
            check_options: true,
            ui: Ui::plain(),
        }
    }

    /// Says what is being done to the source `label` (`Fetching`,
    /// `Installing`), if verbose.
    fn say(&self, verb: &str, label: &str, detail: &str) {
        if self.verbose {
            let line = self
                .ui
                .line(&[(Paint::Dim, verb), (Paint::Name, label), (Paint::Dim, detail)]);
            self.sh.out(line.as_bytes());
        }
    }

    fn problem(&mut self, loc: Option<&Loc>, msg: impl Into<String>) {
        self.problems.push(Problem {
            loc: loc.cloned(),
            msg: msg.into(),
        });
    }

    /// The directory (or file) with the files of `source`, fetched if
    /// needed and allowed. `key` is the name that `plugin update` knows the
    /// source by. `Err` once the problem is recorded.
    fn root(&mut self, source: &Source, key: &str, loc: Option<&Loc>) -> Result<Vec<u8>, ()> {
        let root = match &source.origin {
            Origin::Local(p) => p.clone(),
            Origin::Git { url, at } => {
                let Some(data) = self.data.clone() else {
                    self.problem(None, "no directory for plugins (HOME is not set)");
                    return Err(());
                };
                let fresh = match &self.fetching {
                    Fetching::Update(names) => names.is_empty() || names.iter().any(|n| n == key),
                    _ => false,
                };
                if fresh && !self.updated.iter().any(|n| n == key) {
                    self.updated.push(key.to_string());
                }
                let same = |p: &Pin| p.url == *url && p.at == *at;
                let pinned = match self.used.iter().find(|(p, _)| same(p)) {
                    Some((p, _)) => Some(p.commit.clone()),
                    None if fresh => None,
                    None => self.locked.iter().find(|p| same(p)).map(|p| p.commit.clone()),
                };
                let mut fetched = false;
                let commit = match pinned {
                    Some(c) => c,
                    None if self.fetching == Fetching::No => {
                        self.missing.push(source.label.clone());
                        return Err(());
                    }
                    None if self.interrupted => return Err(()),
                    None => {
                        self.say("Fetching", &source.label, "");
                        match fetch::resolve(self.sh, url, at) {
                            Ok(c) => {
                                fetched = true;
                                c
                            }
                            Err(e) => {
                                self.interrupted |= e == "interrupted";
                                self.problem(loc, format!("{}: {e}", source.label));
                                return Err(());
                            }
                        }
                    }
                };
                if !self.used.iter().any(|(p, _)| same(p)) {
                    let pin = Pin {
                        url: url.clone(),
                        at: at.clone(),
                        commit: commit.clone(),
                    };
                    self.used.push((pin, source.label.clone()));
                }
                let dir = fetch::checkout(&data, url, &commit);
                if !crate::sys::is_dir(&dir) {
                    if self.fetching == Fetching::No {
                        self.missing.push(source.label.clone());
                        return Err(());
                    }
                    if self.interrupted {
                        return Err(());
                    }
                    if !fetched {
                        self.say("Installing", &source.label, &format!("at {}", short(&commit)));
                    }
                    if let Err(e) = fetch::extract(self.sh, &data, url, at, &commit) {
                        self.interrupted |= e == "interrupted";
                        self.problem(loc, format!("{}: {e}", source.label));
                        return Err(());
                    }
                }
                dir
            }
        };
        let root = match &source.subdir {
            Some(s) => [root.as_slice(), b"/", s.as_bytes()].concat(),
            None => root,
        };
        if crate::sys::stat(&root).is_none() {
            let msg = match &source.subdir {
                Some(s) => format!("{}: no directory {s} in it", source.label),
                None => format!("{}: no such file or directory", source.label),
            };
            self.problem(loc, msg);
            return Err(());
        }
        Ok(root)
    }

    /// The plugin `want` in the files `root` of the source `label`: the
    /// source itself if it is one plugin, else the plugin at the path `want`
    /// in the collection, or (with `only`) its only plugin. Also says
    /// whether it is in a collection.
    fn pick(
        &mut self,
        root: &[u8],
        label: &str,
        want: &str,
        only: bool,
        loc: Option<&Loc>,
    ) -> Result<(Found, bool), ()> {
        if !super::is_dir(root) {
            return Ok((super::at_path(root), false));
        }
        if super::is_plugin_dir(root) {
            let found = Found {
                path: root.to_vec(),
                kind: Kind::Dir,
            };
            return Ok((found, false));
        }
        let found = find_path(root, want).or_else(|| match super::main_names(root).as_slice() {
            [one] if only => super::find_path(root, one),
            _ => None,
        });
        match found {
            Some(f) => Ok((f, true)),
            None => {
                self.problem(loc, format!("{want}: no such plugin in {label}"));
                Err(())
            }
        }
    }

    /// The plugin at `path` in the collection `root` of the source `label`,
    /// which messages call `shown`.
    fn find(&mut self, root: &[u8], path: &str, shown: &str, label: &str, loc: Option<&Loc>) -> Result<Found, ()> {
        let msg = match find_path(root, path) {
            // (A directory with no plugins either fails to load, saying so.)
            Some(f)
                if f.kind == Kind::Dir
                    && !super::is_plugin_dir(&f.path)
                    && !super::collection_names(&f.path).is_empty() =>
            {
                format!("{shown}: a collection, not a plugin")
            }
            Some(f) => return Ok(f),
            None => format!("{shown}: no such plugin in {label}"),
        };
        self.problem(loc, msg);
        Err(())
    }

    /// Resolves an entry and its dependencies, from `scope`. `by` says who
    /// asks for it (see [`Resolved`]).
    fn resolve(&mut self, e: &Entry, scope: &Scope, by: &str) -> Result<(), ()> {
        let loc = e.loc.as_ref();
        let name = e.name.as_str();
        let (found, label, root, coll, full) = match (&e.target, scope) {
            (Target::Named(Some(src)), _) => {
                let Some(source) = self.config.named(src) else {
                    self.problem(loc, format!("{src}/{name}: no source called {src}"));
                    return Err(());
                };
                let root = self.root(&source, src, loc)?;
                if !super::is_dir(&root) || super::is_plugin_dir(&root) {
                    self.problem(loc, format!("{src}/{name}: {src} is one plugin, not a collection"));
                    return Err(());
                }
                let found = self.find(&root, name, &format!("{src}/{name}"), src, loc)?;
                let coll = Coll::of(&found, &root, &source.label, Some(src));
                (found, source.label, root, Some(coll), format!("{src}/{name}"))
            }
            (Target::Named(None), Scope::Config) => match self.config.named(name) {
                Some(source) => {
                    let root = self.root(&source, name, loc)?;
                    let (found, member) = self.pick(&root, &source.label, name, true, loc)?;
                    let coll = member.then(|| Coll::of(&found, &root, &source.label, Some(name)));
                    (found, source.label, root, coll, name.to_string())
                }
                None => {
                    let Some(dir) = super::plugin_dir(self.sh) else {
                        self.problem(loc, "no plugin directory (HOME is not set)");
                        return Err(());
                    };
                    let Some(found) = find_in(&dir, name) else {
                        self.problem(loc, format!("{name}: no such plugin"));
                        return Err(());
                    };
                    let coll = Coll::of(&found, &dir, "local", None);
                    (found, "local".to_string(), dir, Some(coll), name.to_string())
                }
            },
            (Target::Named(None), Scope::Collection(c)) => {
                let path = c.path_of(name);
                let found = self.find(&c.root, &path, name, &c.label, loc)?;
                (
                    found,
                    c.label.clone(),
                    c.root.clone(),
                    Some(c.clone()),
                    c.name_of(&path),
                )
            }
            (Target::InSource, Scope::Collection(c)) => {
                let found = self.find(&c.root, name, &format!("/{name}"), &c.label, loc)?;
                let coll = Coll::of(&found, &c.root, &c.label, c.src.as_deref());
                (found, c.label.clone(), c.root.clone(), Some(coll), c.name_of(name))
            }
            (Target::InSource, _) => {
                self.problem(
                    loc,
                    format!("/{name}: only in the plugin.toml of a plugin of a collection"),
                );
                return Err(());
            }
            (Target::Named(None), Scope::None) => {
                self.problem(loc, format!("{name}: not in a collection (write SOURCE/{name})"));
                return Err(());
            }
            (Target::Inline(source, plugin), _) => {
                let root = self.root(source, name, loc)?;
                let want = plugin.as_deref().unwrap_or(name);
                let (found, member) = self.pick(&root, &source.label, want, plugin.is_none(), loc)?;
                let coll = member.then(|| Coll::of(&found, &root, &source.label, None));
                (found, source.label.clone(), root, coll, name.to_string())
            }
        };
        self.add(&full, found, &label, &root, coll, &e.options, by, loc)
    }

    /// Adds a plugin found in the files `root` of its source, after its
    /// dependencies, with the options `given` by `by`. `coll` is the
    /// collection it is in, if any, where plain names in its manifest are.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        name: &str,
        found: Found,
        label: &str,
        root: &[u8],
        coll: Option<Coll>,
        given: &[(String, Given)],
        by: &str,
        loc: Option<&Loc>,
    ) -> Result<(), ()> {
        let abs = super::absolute(self.sh, &found.path);
        if let Some(p) = self.done.iter().find(|p| p.abs == abs) {
            if p.name != name {
                let msg = format!("{name}: the same plugin as {}", p.name);
                self.problem(loc, msg);
                return Err(());
            }
            if !self.check_options {
                return Ok(());
            }
            let msg = match apply(name, &p.decls, given) {
                Ok(o) if o == p.options => return Ok(()),
                Ok(o) => {
                    let (here, there) = differences(&p.decls, &o, &p.options);
                    format!("{name}: inconsistent options: {here} here, but {there} {}", p.by)
                }
                Err(msg) => msg,
            };
            self.problem(loc, msg);
            return Err(());
        }
        if let Some(i) = self.stack.iter().position(|(_, p)| *p == abs) {
            let mut cycle: Vec<&str> = self.stack[i..].iter().map(|(n, _)| n.as_str()).collect();
            cycle.push(name);
            let msg = format!("dependency cycle: {}", cycle.join(" -> "));
            self.problem(loc, msg);
            return Err(());
        }
        let other = self.done.iter().find(|p| p.name == name).map(|p| p.source.clone());
        let other = other.or_else(|| self.stack.iter().any(|(n, _)| n == name).then(String::new));
        if let Some(other) = other {
            let msg = match other.is_empty() {
                true => format!("{name}: two different plugins with this name"),
                false => format!("{name}: two different plugins with this name (from {other} and {label})"),
            };
            self.problem(loc, msg);
            return Err(());
        }
        let rel = match abs.strip_prefix(root).and_then(|r| r.strip_prefix(b"/")) {
            Some(r) if !r.is_empty() => String::from_utf8_lossy(r).into_owned(),
            _ => ".".to_string(),
        };
        let mut deps = Vec::new();
        let mut ok = true;
        let mut decls = Vec::new();
        if found.kind == Kind::Dir {
            self.stack.push((name.to_string(), abs.clone()));
            let file = [abs.as_slice(), b"/plugin.toml"].concat();
            // A git plugin's files are in the data directory, which
            // messages don't show.
            let git = self.data.as_ref().is_some_and(|d| abs.starts_with(d));
            let shown = match git {
                true => format!("{label}:{rel}/plugin.toml").into_bytes(),
                false => file.clone(),
            };
            if !git {
                self.manifests.push(file);
            }
            let entries;
            (entries, decls) = manifest(self.sh, &abs, &shown, &mut self.problems);
            let scope = coll.map_or(Scope::None, Scope::Collection);
            let dependent = format!("for {name}");
            for d in &entries {
                match self.resolve(d, &scope, &dependent) {
                    Ok(()) => deps.push(d.name.clone()),
                    Err(()) => ok = false,
                }
            }
            self.stack.pop();
        }
        let options = match self.check_options {
            true => apply(name, &decls, given),
            false => Ok(Vec::new()),
        };
        let options = match options {
            Ok(o) => o,
            Err(msg) => {
                self.problem(loc, msg);
                return Err(());
            }
        };
        if !ok {
            return Err(());
        }
        self.done.push(Resolved {
            name: name.to_string(),
            found,
            abs,
            source: label.to_string(),
            rel,
            deps,
            decls,
            options,
            by: by.to_string(),
        });
        Ok(())
    }

    /// Records a problem for the sources that aren't installed.
    fn report_missing(&mut self) {
        let mut names = std::mem::take(&mut self.missing);
        names.dedup();
        names.sort();
        names.dedup();
        if !names.is_empty() {
            self.problem(
                None,
                format!("plugin sources not installed: {} (run plugin sync)", names.join(", ")),
            );
        }
    }
}

fn find_in(dir: &[u8], name: &str) -> Option<Found> {
    super::find_in(dir, name.as_bytes())
}

fn find_path(dir: &[u8], path: &str) -> Option<Found> {
    super::find_path(dir, path.as_bytes())
}

/// The lock's pins, or none (with a problem) if it can't be read.
fn locked_pins(sh: &Shell, problems: &mut Vec<Problem>) -> Result<Vec<Pin>, LockError> {
    let Some(file) = lock_path(sh) else {
        return Ok(Vec::new());
    };
    read_lock(&file).inspect_err(|e| {
        problems.push(Problem {
            loc: None,
            msg: format!("{}: {}", String::from_utf8_lossy(&file), e.msg),
        });
    })
}

// ----------------------------------------------------------------------
// Entry points

/// Loads the plugins that `plugins.enabled` lists, and their dependencies,
/// at the start of an interactive shell (from `startcache.rs`, whose cache
/// then records the files read). Returns false if a plugin couldn't be
/// resolved, so that the cache isn't written.
pub fn load_enabled(sh: &mut Shell) -> bool {
    let mut problems = Vec::new();
    // config.rs reports a file that isn't TOML.
    let Ok(config) = read_config(sh, &mut problems) else {
        return true;
    };
    let mut ok = problems.is_empty();
    print_problems(sh, None, &problems);
    if config.enabled.is_empty() {
        return ok;
    }
    let lock = lock_path(sh);
    if let (Some(rec), Some(lock)) = (&mut sh.sourced_files, lock) {
        rec.push(lock);
    }
    let mut problems = Vec::new();
    let pins = locked_pins(sh, &mut problems).unwrap_or_default();
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    r.problems = problems;
    for e in &config.enabled {
        let _ = r.resolve(e, &Scope::Config, ENABLED);
    }
    r.report_missing();
    let (done, problems, manifests) = (r.done, r.problems, r.manifests);
    if let Some(rec) = &mut sh.sourced_files {
        rec.extend(manifests);
    }
    print_problems(sh, None, &problems);
    ok &= problems.is_empty();
    for p in done {
        let _ = super::load_found(sh, b"plugin", &p.found, Some(p.name.into_bytes()), p.options, true);
    }
    ok
}

/// Who gives the options of an entry of `plugins.enabled`, and of `plugin
/// load` (see [`Resolved`]).
const ENABLED: &str = "in plugins.enabled";
const LOADED: &str = "in plugin load";

/// The options of the plugin `found`, loaded as `name`, given `opts`
/// (`OPTION=VALUE` for `plugin restore`), with the defaults of the others.
pub(super) fn given_options(
    sh: &Shell,
    found: &Found,
    name: &str,
    opts: super::OptionArgs,
) -> Result<PluginOptions, String> {
    let decls = match found.kind {
        Kind::Dir => manifest(sh, &found.path, &found.path, &mut Vec::new()).1,
        _ => Vec::new(),
    };
    let given: Vec<_> = opts.into_iter().map(|(k, v)| (k, Given::Text(v))).collect();
    apply(name, &decls, &given)
}

/// `plugin load ARG OPTION=VALUE...`: a plugin of a named source
/// (`SOURCE/PATH`, or `NAME`), else a plugin in the plugin directory or a
/// path, with the options `opts`, after its dependencies that aren't loaded
/// yet.
pub fn load(sh: &mut Shell, cmd: &[u8], arg: &[u8], opts: super::OptionArgs) -> ExecResult {
    let given: Vec<_> = opts.into_iter().map(|(k, v)| (k, Given::Text(v))).collect();
    // Errors in the table are for `plugin sync` and startup to report.
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let text = String::from_utf8_lossy(arg);
    let named = match text.split_once('/') {
        Some((src, path)) => config.named(src).is_some() && valid_path(path),
        None => config.named(&text).is_some(),
    };
    // A lock that can't be read shows as plugins that aren't installed.
    let pins = read_lock(&lock_path(sh).unwrap_or_default()).unwrap_or_default();
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let ok = if named {
        let (source, name) = match text.split_once('/') {
            Some((s, n)) => (Some(s.to_string()), n.to_string()),
            None => (None, text.to_string()),
        };
        let e = Entry {
            name,
            target: Target::Named(source),
            options: given,
            loc: None,
        };
        r.resolve(&e, &Scope::Config, LOADED).is_ok()
    } else {
        let Some(found) = super::find(r.sh, arg) else {
            r.sh.berr(cmd, "no plugin directory (HOME is not set)");
            return Ok(1);
        };
        if crate::sys::stat(&found.path).is_none() {
            // For its error.
            return super::load_found(r.sh, cmd, &found, None, Vec::new(), true);
        }
        let name = String::from_utf8_lossy(&super::plugin_name(&found.path, found.kind)).into_owned();
        let abs = super::absolute(r.sh, &found.path);
        let dir = match arg.contains(&b'/') {
            true => parent(&abs).to_vec(),
            false => super::plugin_dir(r.sh).unwrap_or_default(),
        };
        let label = if arg.contains(&b'/') {
            text.to_string()
        } else {
            "local".into()
        };
        let coll = Coll {
            dir: dir.clone(),
            root: dir.clone(),
            label: label.clone(),
            src: None,
        };
        r.add(&name, found, &label, &dir, Some(coll), &given, LOADED, None)
            .is_ok()
    };
    load_resolved(cmd, r, ok)
}

/// Loads the plugins that `r` resolved (`ok` if it could), those that
/// aren't loaded yet and the last one.
fn load_resolved(cmd: &[u8], mut r: Resolver, ok: bool) -> ExecResult {
    r.report_missing();
    let (sh, done, problems) = (r.sh, r.done, r.problems);
    print_problems(sh, Some(cmd), &problems);
    if !ok {
        return Ok(1);
    }
    let loaded = super::loaded_names(sh);
    // A plugin loaded already must have been given the same options.
    if let Some(host) = sh.plugins.clone() {
        for p in &done {
            if let Some(old) = host.options(p.name.as_bytes()).filter(|o| *o != p.options) {
                let (here, there) = differences(&p.decls, &p.options, &old);
                let name = &p.name;
                let msg = format!(
                    "{name}: inconsistent options: {here} here, but loaded with {there} (plugin unload {name} first)"
                );
                sh.berr(cmd, msg);
                return Ok(1);
            }
        }
    }
    let mut status = 0;
    let last = done.len().saturating_sub(1);
    let ui = super::confirms(sh).then(|| Ui::new(sh));
    for (i, p) in done.into_iter().enumerate() {
        let name = p.name.into_bytes();
        if i == last || !loaded.contains(&name) {
            let st = super::load_found(sh, cmd, &p.found, Some(name.clone()), p.options, true)?;
            status = st.max(status);
            if let (0, Some(ui)) = (st, &ui) {
                let again = loaded.contains(&name);
                let verb = if again { "Reloaded" } else { "Loaded" };
                let why = if i == last { "" } else { "(a dependency)" };
                let name = String::from_utf8_lossy(&name);
                let line = ui.line(&[(Paint::Ok, verb), (Paint::Name, &name), (Paint::Dim, why)]);
                sh.out(line.as_bytes());
            }
        }
    }
    Ok(status)
}

/// For `plugin add`: loads the plugin `name` of `plugins.enabled`.
pub(super) fn load_added(sh: &mut Shell, cmd: &[u8], name: &str) -> ExecResult {
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let Some(e) = config.enabled.iter().find(|e| e.key() == name) else {
        return Ok(0);
    };
    let pins = read_lock(&lock_path(sh).unwrap_or_default()).unwrap_or_default();
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let ok = r.resolve(e, &Scope::Config, ENABLED).is_ok();
    load_resolved(cmd, r, ok)
}

/// For `plugin add`: the keys in `plugins.enabled` (with `/` for TOML's
/// dots) and the names in
/// `plugins.available` of the text `bytes` of the configuration file
/// `file`, or its first problem.
pub(super) fn config_names(sh: &Shell, file: &[u8], bytes: &[u8]) -> Result<(Vec<String>, Vec<String>), String> {
    let mut problems = Vec::new();
    let config = parse_config(sh, file, bytes, &mut problems).map_err(|p| problem_text(&p))?;
    if let Some(p) = problems.first() {
        return Err(problem_text(p));
    }
    Ok((
        config.enabled.iter().map(Entry::key).collect(),
        config.available.into_iter().map(|(n, _)| n).collect(),
    ))
}

fn problem_text(p: &Problem) -> String {
    match &p.loc {
        Some(l) => format!("{}: line {}: {}", String::from_utf8_lossy(&l.file), l.line, p.msg),
        None => p.msg.clone(),
    }
}

/// For `plugin add`: whether `name` is a source (in `plugins.available`,
/// or `std`).
pub(super) fn is_named_source(sh: &Shell, name: &str) -> bool {
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    config.named(name).is_some()
}

/// The first 7 characters of a commit hash, as messages show it.
fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// `N thing` or `N things`.
fn count(n: usize, thing: &str) -> String {
    match n {
        1 => format!("1 {thing}"),
        _ => format!("{n} {thing}s"),
    }
}

/// Reads `config.toml` and `plugins.lock` for `plugin sync`, `plugin update`
/// and `plugin check`: the configuration, the lock's path and its pins, or
/// the status once the problems are reported. A broken lock is reported and
/// gives no pins.
fn read_state(sh: &mut Shell, cmd: &[u8]) -> Result<(Config, Vec<u8>, Vec<Pin>), i32> {
    let mut problems = Vec::new();
    let config = match read_config(sh, &mut problems) {
        Ok(c) => c,
        Err(p) => {
            print_problems(sh, Some(cmd), &[p]);
            return Err(1);
        }
    };
    if !problems.is_empty() {
        print_problems(sh, Some(cmd), &problems);
        return Err(1);
    }
    let Some(lock) = lock_path(sh) else {
        sh.berr(cmd, "no configuration directory (HOME is not set)");
        return Err(1);
    };
    let pins = match locked_pins(sh, &mut problems) {
        Ok(pins) => pins,
        Err(e) if e.newer => {
            print_problems(sh, Some(cmd), &problems);
            return Err(1);
        }
        Err(_) => {
            print_problems(sh, Some(cmd), &problems);
            Vec::new()
        }
    };
    Ok((config, lock, pins))
}

/// Resolves the enabled plugins, then the plugins of the available sources
/// (so that `plugin load` finds them and their dependencies). Gives the
/// enabled plugins, and whether resolving them had problems.
fn resolve_all(r: &mut Resolver, config: &Config) -> (Vec<Resolved>, bool) {
    for e in &config.enabled {
        let _ = r.resolve(e, &Scope::Config, ENABLED);
    }
    let enabled = std::mem::take(&mut r.done);
    let failed = !r.problems.is_empty();
    // The others are installed, but not loaded: options are for loading.
    r.check_options = false;
    for (name, source) in &config.available {
        if r.interrupted {
            break;
        }
        let Ok(root) = r.root(source, name, None) else {
            continue;
        };
        let names = match super::is_dir(&root) && !super::is_plugin_dir(&root) {
            true => super::collection_names(&root)
                .into_iter()
                .map(|n| (Some(name.clone()), String::from_utf8_lossy(&n).into_owned()))
                .collect(),
            false => vec![(None, name.clone())],
        };
        for (source, plugin) in names {
            let e = Entry {
                name: plugin,
                target: Target::Named(source),
                options: Vec::new(),
                loc: None,
            };
            let _ = r.resolve(&e, &Scope::Config, ENABLED);
            r.done.clear();
        }
    }
    (enabled, failed)
}

/// The commits' comparison on GitHub, for a source there.
fn compare_url(url: &str, old: &str, new: &str) -> Option<String> {
    let repo = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("git@github.com:"))?;
    let repo = repo.trim_end_matches('/');
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    Some(format!(
        "https://github.com/{repo}/compare/{}...{}",
        short(old),
        short(new)
    ))
}

/// ` (a, b)` for the plugins of `enabled` that come from the source `label`.
fn provides(enabled: &[Resolved], label: &str) -> String {
    let names: Vec<&str> = (enabled.iter())
        .filter(|p| p.source == label)
        .map(|p| p.name.as_str())
        .collect();
    match names.is_empty() {
        true => String::new(),
        false => format!("({})", names.join(", ")),
    }
}

/// `plugin sync` (`update` is `None`) and `plugin update [NAME...]`: resolves
/// the enabled plugins and the plugins of the available sources, fetching
/// what is missing (and, for `update`, the newest commits), and writes
/// `plugins.lock`. Unless `quiet`, says what it fetches, the sources whose
/// commits changed (with a link to see the changes, for GitHub's), and what
/// the lock holds.
pub fn sync(sh: &mut Shell, cmd: &[u8], update: Option<&[Vec<u8>]>, quiet: bool) -> ExecResult {
    let (config, lock, old) = match read_state(sh, cmd) {
        Ok(state) => state,
        Err(status) => return Ok(status),
    };
    let fetching = match update {
        None => Fetching::Missing,
        Some(names) => Fetching::Update(names.iter().map(|n| String::from_utf8_lossy(n).into_owned()).collect()),
    };
    let ui = if quiet { Ui::plain() } else { Ui::new(sh) };
    let mut r = Resolver::new(sh, &config, old.clone(), fetching);
    r.verbose = !quiet;
    r.ui = ui;
    let (enabled, mut failed) = resolve_all(&mut r, &config);
    if let Fetching::Update(names) = &r.fetching {
        for n in names {
            if !r.updated.contains(n) {
                r.problems.push(Problem {
                    loc: None,
                    msg: format!("{n}: no such git source"),
                });
                failed = true;
            }
        }
    }
    failed |= r.interrupted;
    let (used, problems, ui) = (r.used, r.problems, r.ui);
    print_problems(sh, Some(cmd), &problems);
    if failed {
        return Ok(1);
    }
    let mut out = String::new();
    let mut moved = 0;
    for (pin, label) in &used {
        let what = provides(&enabled, label);
        match old.iter().find(|p| p.url == pin.url && p.at == pin.at) {
            None => {
                let at = format!("at {}", short(&pin.commit));
                out.push_str(&ui.line(&[
                    (Paint::Ok, "Locking"),
                    (Paint::Name, label),
                    (Paint::Dim, &at),
                    (Paint::Dim, &what),
                ]));
            }
            Some(p) if p.commit != pin.commit => {
                moved += 1;
                let range = format!("{}..{}", short(&p.commit), short(&pin.commit));
                out.push_str(&ui.line(&[
                    (Paint::Update, "Updating"),
                    (Paint::Name, label),
                    (Paint::Dim, &range),
                    (Paint::Dim, &what),
                ]));
                if let Some(link) = compare_url(&pin.url, &p.commit, &pin.commit) {
                    out.push_str(&format!("   {}\n", ui.paint(Paint::Dim, &link)));
                }
            }
            Some(_) if update.is_some() => {
                let at = format!("at {}", short(&pin.commit));
                out.push_str(&ui.line(&[
                    (Paint::Ok, "Up to date"),
                    (Paint::Name, label),
                    (Paint::Dim, &at),
                    (Paint::Dim, &what),
                ]));
            }
            Some(_) => {}
        }
    }
    let pins: Vec<Pin> = used.into_iter().map(|(p, _)| p).collect();
    let empty = pins.is_empty() && enabled.is_empty();
    if empty && crate::sys::stat(&lock).is_none() {
        if !quiet {
            sh.out(
                ui.line(&[(Paint::Dim, "No plugins enabled and no git sources")])
                    .as_bytes(),
            );
        }
        return Ok(0);
    }
    if let Err(e) = write_file(&lock, &lock_text(&pins, &enabled)) {
        sh.berr(cmd, e);
        return Ok(1);
    }
    let status = match quiet {
        true => 0,
        false => {
            let mut summary = format!("{} locked", count(pins.len(), "git source"));
            if update.is_some() {
                match moved {
                    0 => summary.push_str(" (none updated)"),
                    n => summary.push_str(&format!(" ({n} updated)")),
                }
            }
            let line = match enabled.is_empty() {
                true => format!("{summary}, no plugins enabled\n"),
                false => {
                    let names: Vec<String> = enabled.iter().map(|p| ui.paint(Paint::Name, &p.name)).collect();
                    format!(
                        "{summary}, {} enabled: {}\n",
                        count(names.len(), "plugin"),
                        names.join(", ")
                    )
                }
            };
            out.push_str(&line);
            sh.out_status(out.as_bytes())
        }
    };
    Ok(status.max(if problems.is_empty() { 0 } else { 1 }))
}

/// `plugin check [-q]`: says, for each git source, whether it has a newer
/// commit than the one in `plugins.lock` (asking with `git ls-remote`), and
/// which aren't installed, without fetching or changing anything. With
/// `quiet` only the sources that can be updated or aren't installed are
/// listed. The status is 0 unless something couldn't be checked.
pub fn check(sh: &mut Shell, cmd: &[u8], quiet: bool) -> ExecResult {
    let (config, _, pins) = match read_state(sh, cmd) {
        Ok(state) => state,
        Err(status) => return Ok(status),
    };
    let ui = Ui::new(sh);
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let (enabled, _) = resolve_all(&mut r, &config);
    let (used, problems, mut missing) = (r.used, r.problems, r.missing);
    print_problems(sh, Some(cmd), &problems);
    let mut status = if problems.is_empty() { 0 } else { 1 };
    let (mut checked, mut newer, mut fixed) = (0, 0, 0);
    for (pin, label) in &used {
        let what = provides(&enabled, label);
        let at = format!("at {}", short(&pin.commit));
        // A commit given with `rev` has nothing newer.
        if matches!(pin.at, GitRef::Rev(_)) {
            checked += 1;
            fixed += 1;
            if !quiet {
                let line = ui.line(&[
                    (Paint::Ok, "Pinned:"),
                    (Paint::Name, label),
                    (Paint::Dim, &at),
                    (Paint::Dim, &what),
                ]);
                sh.out(line.as_bytes());
            }
            continue;
        }
        match fetch::remote_commit(sh, &pin.url, &pin.at) {
            Ok(c) if c == pin.commit => {
                checked += 1;
                if !quiet {
                    let line = ui.line(&[
                        (Paint::Ok, "Up to date:"),
                        (Paint::Name, label),
                        (Paint::Dim, &at),
                        (Paint::Dim, &what),
                    ]);
                    sh.out(line.as_bytes());
                }
            }
            Ok(c) => {
                checked += 1;
                newer += 1;
                let range = format!("{}..{}", short(&pin.commit), short(&c));
                let mut line = ui.line(&[
                    (Paint::Update, "Update available:"),
                    (Paint::Name, label),
                    (Paint::Dim, &range),
                    (Paint::Dim, &what),
                ]);
                if let Some(link) = compare_url(&pin.url, &pin.commit, &c) {
                    line.push_str(&format!("   {}\n", ui.paint(Paint::Dim, &link)));
                }
                sh.out(line.as_bytes());
            }
            Err(e) => {
                sh.berr(cmd, format!("{label}: {e}"));
                status = 1;
                if e == "interrupted" {
                    return Ok(status);
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        let list: Vec<String> = missing.iter().map(|m| ui.paint(Paint::Name, m)).collect();
        let line = ui.line(&[
            (Paint::Warn, "Not installed:"),
            (Paint::Dim, &list.join(", ")),
            (Paint::Dim, "(run plugin sync)"),
        ]);
        sh.out(line.as_bytes());
    }
    if quiet {
        return Ok(status);
    }
    let sources = count(checked, "git source");
    let pinned = match fixed {
        0 => String::new(),
        n => format!(", {n} pinned to a commit"),
    };
    let msg = match newer {
        0 if checked == 0 => ui.line(&[(Paint::Dim, "No git sources to check")]),
        0 => ui.line(&[(Paint::Ok, &format!("{sources} up to date{pinned}"))]),
        _ => ui.line(&[(
            Paint::Update,
            &format!("{newer} of {sources} can be updated{pinned} (run plugin update)"),
        )]),
    };
    sh.out(msg.as_bytes());
    Ok(status)
}

/// The path of the plugin that `plugin load ARG` loads from a source (`ARG`
/// a source, or `SOURCE/PATH`), if it is installed. For `plugin unload`.
pub fn location(sh: &mut Shell, arg: &[u8]) -> Option<Vec<u8>> {
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let text = String::from_utf8_lossy(arg);
    let (src, name, only) = match text.split_once('/') {
        Some((src, path)) if valid_path(path) => (src, path, false),
        Some(_) => return None,
        None => (&*text, &*text, true),
    };
    let source = config.named(src)?;
    let pins = read_lock(&lock_path(sh).unwrap_or_default()).unwrap_or_default();
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let root = r.root(&source, src, None).ok()?;
    r.pick(&root, src, name, only, None).ok().map(|(found, _)| found.path)
}

/// The plugins of the available sources that are installed, for `plugin
/// list-available`: `SOURCE` for a source that is one plugin, and
/// `SOURCE/PATH` for each plugin of a collection, with the plugin's path.
pub fn available(sh: &mut Shell) -> Vec<(Vec<u8>, Vec<u8>)> {
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let pins = read_lock(&lock_path(sh).unwrap_or_default()).unwrap_or_default();
    let mut names: Vec<String> = config.available.iter().map(|(n, _)| n.clone()).collect();
    if !names.iter().any(|n| n == "std") {
        names.push("std".into());
    }
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let mut out = Vec::new();
    for name in names {
        let Some(source) = r.config.named(&name) else { continue };
        let Ok(root) = r.root(&source, &name, None) else {
            continue;
        };
        if super::is_dir(&root) && !super::is_plugin_dir(&root) {
            for p in super::collection_names(&root) {
                if let Some(found) = super::find_path(&root, &p) {
                    out.push((found.path, [name.as_bytes(), b"/", &p].concat()));
                }
            }
        } else {
            out.push((root, name.into_bytes()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Config, GitRef, Origin, Pin, compare_url, lock_text, parent, toml_str};

    #[test]
    fn parent_of_odd_paths() {
        assert_eq!(parent(b""), b"");
        assert_eq!(parent(b"a"), b"a");
        assert_eq!(parent(b"/"), b"/");
        assert_eq!(parent(b"/a"), b"/");
        assert_eq!(parent(b"/a/b"), b"/a");
    }

    #[test]
    fn compare_links() {
        let (a, b) = ("0123456789abcdef", "fedcba9876543210");
        let link = Some("https://github.com/o/r/compare/0123456...fedcba9".to_string());
        for url in [
            "https://github.com/o/r",
            "https://github.com/o/r.git",
            "git@github.com:o/r.git",
        ] {
            assert_eq!(compare_url(url, a, b), link, "{url}");
        }
        assert_eq!(compare_url("file:///x/r", a, b), None);
        assert_eq!(compare_url("https://example.org/o/r", a, b), None);
    }

    #[test]
    fn std_is_this_release() {
        let std = Config::default().named("std").unwrap();
        let Origin::Git { url, at } = std.origin else {
            panic!("std isn't a git source");
        };
        assert_eq!(url, "https://github.com/luispedro/luish.git");
        assert_eq!(at, GitRef::Tag(format!("v{}", env!("CARGO_PKG_VERSION"))));
        assert_eq!(std.subdir.as_deref(), Some("luish-std-plugins"));
    }

    #[test]
    fn strings() {
        assert_eq!(toml_str("a\"b\\c\n"), r#""a\"b\\c\u000A""#);
    }

    #[test]
    fn lock_layout() {
        let pin = |url: &str, at| Pin {
            url: url.into(),
            at,
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
        };
        let text = lock_text(&[pin("b", GitRef::Branch("main".into())), pin("a", GitRef::Head)], &[]);
        assert_eq!(
            text,
            "# Written by luish (plugin sync, plugin update). Don't edit.\nversion = 1\n\
             \n[[source]]\nurl = \"a\"\ncommit = \"0123456789abcdef0123456789abcdef01234567\"\n\
             \n[[source]]\nurl = \"b\"\nbranch = \"main\"\ncommit = \"0123456789abcdef0123456789abcdef01234567\"\n"
        );
    }
}
