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
//! plugins (`NAME.rhai`, `NAME.lsh`, `NAME/`). `plugins.available` names
//! sources, and `std` names luish's own collection. `plugins.enabled` lists
//! the plugins that interactive shells load, as `SOURCE/NAME` (a plugin of a
//! collection), `NAME` (a named source, else a plugin in the plugin
//! directory) or `NAME = { gh = ... }` (a source of its own). A directory
//! plugin's `plugin.toml` lists its dependencies in the same way, where a
//! plain `NAME` is a plugin of the same collection.
//!
//! `plugin sync` resolves all of these (fetching with git, `fetch.rs`) and
//! writes `plugins.lock`, which pins each git source to a commit; startup and
//! `plugin load` resolve with the pins, without the network.

use super::{Found, Kind, fetch};
use crate::config::{in_order, line_of, report, tilde};
use crate::interactive::to_path;
use crate::shell::{ExecResult, Shell};
use toml_span::value::{Table, Value, ValueInner};

/// `std`: the collection in luish's repository.
const STD_REPO: &str = "luispedro/luish";
const STD_SUBDIR: &str = "luish-std-plugins";

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
    /// `NAME = "*"` (`None`) or `SOURCE/NAME = "*"`.
    Named(Option<String>),
    /// `NAME = { gh = ..., plugin = ... }`.
    Inline(Source, Option<String>),
}

/// An entry of `plugins.enabled` or of a manifest's `dependencies`.
#[derive(Clone, Debug)]
struct Entry {
    /// The plugin's name: the last part of the key.
    name: String,
    target: Target,
    loc: Option<Loc>,
}

/// The `[plugins]` table.
#[derive(Default)]
struct Config {
    available: Vec<(String, Source)>,
    enabled: Vec<Entry>,
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
                at: GitRef::Head,
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

fn github_url(repo: &str) -> String {
    format!("https://github.com/{repo}.git")
}

/// A plugin's or a source's name: not empty, without `/`, not starting
/// with `.`.
fn valid_name(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('.') && !s.contains(['/', '\0', '\n'])
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
                    if !self.requirement(start, key, req) {
                        continue;
                    }
                    let (source, name) = match key.split_once('/') {
                        Some((s, n)) => (Some(s), n),
                        None => (None, key),
                    };
                    if !valid_name(name) || source.is_some_and(|s| !valid_name(s)) {
                        self.err(start, format!("{key}: bad plugin name (NAME or SOURCE/NAME)"));
                        continue;
                    }
                    out.push(Entry {
                        name: name.into(),
                        target: Target::Named(source.map(Into::into)),
                        loc: Some(self.loc(start)),
                    });
                }
                ValueInner::Table(t) if is_source(t) => {
                    if !valid_name(key) {
                        self.err(start, format!("{key}: bad plugin name"));
                    } else if let Some((source, plugin)) = self.source(key, &value, true) {
                        out.push(Entry {
                            name: key.into(),
                            target: Target::Inline(source, plugin),
                            loc: Some(self.loc(start)),
                        });
                    }
                }
                // `SOURCE.NAME = "*"`, which TOML reads as a table.
                ValueInner::Table(t) => {
                    if !valid_name(key) {
                        self.err(start, format!("{key}: bad source name"));
                        continue;
                    }
                    let mut plugins: Vec<_> = t.iter().collect();
                    plugins.sort_by_key(|(k, _)| k.span.start);
                    for (k, v) in plugins {
                        let name = &*k.name;
                        let full = format!("{key}.{name}");
                        match v.as_ref() {
                            ValueInner::String(req) if self.requirement(k.span.start, &full, req) => {
                                if !valid_name(name) {
                                    self.err(k.span.start, format!("{full}: bad plugin name"));
                                    continue;
                                }
                                out.push(Entry {
                                    name: name.into(),
                                    target: Target::Named(Some(key.into())),
                                    loc: Some(self.loc(k.span.start)),
                                });
                            }
                            ValueInner::String(_) => {}
                            _ => self.err(k.span.start, format!("{full}: expected \"*\"")),
                        }
                    }
                }
                other => self.err(
                    start,
                    format!("{key}: expected \"*\" or a table, found {}", other.type_str()),
                ),
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
        if let Some(p) = which.as_ref().filter(|p| !valid_name(p)) {
            self.err(start, format!("{name}.plugin: bad plugin name {p:?}"));
            return None;
        }
        Some((Source { origin, subdir, label }, which))
    }
}

/// Whether a table in `plugins.enabled` or `dependencies` is a source (and
/// not `SOURCE.NAME = "*"`).
fn is_source(t: &Table<'_>) -> bool {
    t.keys().any(|k| ["gh", "git", "path"].contains(&&*k.name))
}

/// Parses the TOML file `file` and gives its top-level table to `f`.
/// `Ok(None)` if the file doesn't exist. Messages call it `shown`.
fn with_toml<R>(file: &[u8], shown: &[u8], f: impl FnOnce(&str, Table<'_>) -> R) -> Result<Option<R>, Problem> {
    let Ok(bytes) = std::fs::read(to_path(file)) else {
        return Ok(None);
    };
    let problem = |offset: usize, msg: String| Problem {
        loc: Some(Loc {
            file: shown.to_vec(),
            line: line_of(&bytes, offset),
        }),
        msg,
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| problem(0, "not valid UTF-8".into()))?;
    let mut root = toml_span::parse(text).map_err(|e| problem(e.span.start, e.to_string()))?;
    match root.take() {
        ValueInner::Table(t) => Ok(Some(f(text, t))),
        _ => Ok(None),
    }
}

fn parent(path: &[u8]) -> &[u8] {
    &path[..path.iter().rposition(|&c| c == b'/').unwrap_or(0).max(1)]
}

/// Reads the `[plugins]` table of `config.toml`: an empty table if there is
/// none, or `Err` if the file isn't TOML. Errors in the table go to
/// `problems`.
fn read_config(sh: &Shell, problems: &mut Vec<Problem>) -> Result<Config, Problem> {
    let Some(file) = crate::config::path(sh) else {
        return Ok(Config::default());
    };
    let config = with_toml(&file, &file, |text, mut root| {
        let Some(value) = root.remove("plugins") else {
            return Config::default();
        };
        let mut r = Reader {
            sh,
            file: &file,
            text,
            base: parent(&file),
            problems,
        };
        r.plugins(value)
    })?;
    Ok(config.unwrap_or_default())
}

/// The dependencies in `plugin.toml`, the manifest of the directory plugin
/// `dir`, which messages call `shown`. The manifest's other keys (such as
/// `description`) are ignored.
fn manifest(sh: &Shell, dir: &[u8], shown: &[u8], problems: &mut Vec<Problem>) -> Vec<Entry> {
    let file = [dir, b"/plugin.toml"].concat();
    let r = with_toml(&file, shown, |text, mut root| {
        let mut out = Vec::new();
        if let Some(value) = root.remove("dependencies") {
            let mut r = Reader {
                sh,
                file: shown,
                text,
                base: dir,
                problems,
            };
            r.entries(value, "dependencies", &mut out);
        }
        out
    });
    match r {
        Ok(entries) => entries.unwrap_or_default(),
        Err(p) => {
            problems.push(p);
            Vec::new()
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

/// A TOML basic string.
fn toml_str(s: &str) -> String {
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
fn write_file(file: &[u8], text: &str) -> Result<(), String> {
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
}

/// Where a plain `NAME` in a manifest's `dependencies` is looked for.
#[derive(Clone)]
enum Scope {
    /// In `plugins.enabled` and for `plugin load NAME`: a named source, else
    /// the plugin directory.
    Config,
    /// In the collection `dir` of the source `label`, whose files are in
    /// `root`.
    Collection { dir: Vec<u8>, root: Vec<u8>, label: String },
    /// Nowhere: the plugin isn't in a collection.
    None,
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
        }
    }

    /// Prints a progress message, if verbose.
    fn say(&self, msg: String) {
        if self.verbose {
            self.sh.out(msg.as_bytes());
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
                        self.say(format!("Fetching {}\n", source.label));
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
                        self.say(format!("Installing {} at {}\n", source.label, short(&commit)));
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
    /// source itself if it is one plugin, else the plugin `want` of the
    /// collection, or (with `only`) its only plugin. Gives the collection
    /// too, for the plugin's dependencies.
    fn pick(&mut self, root: &[u8], label: &str, want: &str, only: bool, loc: Option<&Loc>) -> Result<Picked, ()> {
        if !super::is_dir(root) {
            return Ok((super::at_path(root), None));
        }
        if super::is_plugin_dir(root) {
            let found = Found {
                path: root.to_vec(),
                kind: Kind::Dir,
            };
            return Ok((found, None));
        }
        let found = find_in(root, want).or_else(|| match super::available_names(root).as_slice() {
            [one] if only => find_in(root, &String::from_utf8_lossy(one)),
            _ => None,
        });
        match found {
            Some(f) => Ok((f, Some(root.to_vec()))),
            None => {
                self.problem(loc, format!("{want}: no such plugin in {label}"));
                Err(())
            }
        }
    }

    /// Resolves an entry and its dependencies, from `scope`.
    fn resolve(&mut self, e: &Entry, scope: &Scope) -> Result<(), ()> {
        let loc = e.loc.as_ref();
        let name = e.name.as_str();
        let (found, label, root, collection) = match (&e.target, scope) {
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
                let Some(found) = find_in(&root, name) else {
                    self.problem(loc, format!("{src}/{name}: no such plugin in {src}"));
                    return Err(());
                };
                (found, source.label, root.clone(), Some(root))
            }
            (Target::Named(None), Scope::Config) => match self.config.named(name) {
                Some(source) => {
                    let root = self.root(&source, name, loc)?;
                    let (found, coll) = self.pick(&root, &source.label, name, true, loc)?;
                    (found, source.label, root, coll)
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
                    (found, "local".to_string(), dir.clone(), Some(dir))
                }
            },
            (Target::Named(None), Scope::Collection { dir, root, label }) => {
                let Some(found) = find_in(dir, name) else {
                    self.problem(loc, format!("{name}: no such plugin in {label}"));
                    return Err(());
                };
                (found, label.clone(), root.clone(), Some(dir.clone()))
            }
            (Target::Named(None), Scope::None) => {
                self.problem(loc, format!("{name}: not in a collection (write SOURCE/{name})"));
                return Err(());
            }
            (Target::Inline(source, plugin), _) => {
                let root = self.root(source, name, loc)?;
                let want = plugin.as_deref().unwrap_or(name);
                let (found, coll) = self.pick(&root, &source.label, want, plugin.is_none(), loc)?;
                (found, source.label.clone(), root, coll)
            }
        };
        self.add(name, found, &label, &root, collection, loc)
    }

    /// Adds a plugin found in the files `root` of its source, after its
    /// dependencies. `collection` is where plain names in its manifest are.
    fn add(
        &mut self,
        name: &str,
        found: Found,
        label: &str,
        root: &[u8],
        collection: Option<Vec<u8>>,
        loc: Option<&Loc>,
    ) -> Result<(), ()> {
        let abs = super::absolute(self.sh, &found.path);
        if let Some(p) = self.done.iter().find(|p| p.abs == abs) {
            if p.name == name {
                return Ok(());
            }
            let msg = format!("{name}: the same plugin as {}", p.name);
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
            let entries = manifest(self.sh, &abs, &shown, &mut self.problems);
            let scope = match collection {
                Some(dir) => Scope::Collection {
                    dir,
                    root: root.to_vec(),
                    label: label.to_string(),
                },
                None => Scope::None,
            };
            for d in &entries {
                match self.resolve(d, &scope) {
                    Ok(()) => deps.push(d.name.clone()),
                    Err(()) => ok = false,
                }
            }
            self.stack.pop();
        }
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

type Picked = (Found, Option<Vec<u8>>);

fn find_in(dir: &[u8], name: &str) -> Option<Found> {
    super::find_in(dir, name.as_bytes())
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
        let _ = r.resolve(e, &Scope::Config);
    }
    r.report_missing();
    let (done, problems, manifests) = (r.done, r.problems, r.manifests);
    if let Some(rec) = &mut sh.sourced_files {
        rec.extend(manifests);
    }
    print_problems(sh, None, &problems);
    ok &= problems.is_empty();
    for p in done {
        let _ = super::load_found(sh, b"plugin", &p.found, Some(p.name.into_bytes()), true);
    }
    ok
}

/// `plugin load ARG`: a plugin of a named source (`SOURCE/NAME`, or
/// `NAME`), else a plugin in the plugin directory or a path, after its
/// dependencies that aren't loaded yet.
pub fn load(sh: &mut Shell, cmd: &[u8], arg: &[u8]) -> ExecResult {
    // Errors in the table are for `plugin sync` and startup to report.
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let text = String::from_utf8_lossy(arg);
    let named = match text.split_once('/') {
        Some((src, name)) => config.named(src).is_some() && valid_name(name),
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
            loc: None,
        };
        r.resolve(&e, &Scope::Config).is_ok()
    } else {
        let Some(found) = super::find(r.sh, arg) else {
            r.sh.berr(cmd, "no plugin directory (HOME is not set)");
            return Ok(1);
        };
        if crate::sys::stat(&found.path).is_none() {
            // For its error.
            return super::load_found(r.sh, cmd, &found, None, true);
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
        r.add(&name, found, &label, &dir.clone(), Some(dir), None).is_ok()
    };
    r.report_missing();
    let (done, problems) = (r.done, r.problems);
    print_problems(sh, Some(cmd), &problems);
    if !ok {
        return Ok(1);
    }
    let loaded = super::loaded_names(sh);
    let mut status = 0;
    let last = done.len().saturating_sub(1);
    for (i, p) in done.into_iter().enumerate() {
        let name = p.name.into_bytes();
        if i == last || !loaded.contains(&name) {
            status = super::load_found(sh, cmd, &p.found, Some(name), true)?.max(status);
        }
    }
    Ok(status)
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
        let _ = r.resolve(e, &Scope::Config);
    }
    let enabled = std::mem::take(&mut r.done);
    let failed = !r.problems.is_empty();
    for (name, source) in &config.available {
        if r.interrupted {
            break;
        }
        let Ok(root) = r.root(source, name, None) else {
            continue;
        };
        let names = match super::is_dir(&root) && !super::is_plugin_dir(&root) {
            true => super::available_names(&root)
                .into_iter()
                .map(|n| (Some(name.clone()), String::from_utf8_lossy(&n).into_owned()))
                .collect(),
            false => vec![(None, name.clone())],
        };
        for (source, plugin) in names {
            let e = Entry {
                name: plugin,
                target: Target::Named(source),
                loc: None,
            };
            let _ = r.resolve(&e, &Scope::Config);
            r.done.clear();
        }
    }
    (enabled, failed)
}

/// `plugin sync` (`update` is `None`) and `plugin update [NAME...]`: resolves
/// the enabled plugins and the plugins of the available sources, fetching
/// what is missing (and, for `update`, the newest commits), and writes
/// `plugins.lock`. Unless `quiet`, says what it fetches, the sources whose
/// commits changed, and what the lock holds.
pub fn sync(sh: &mut Shell, cmd: &[u8], update: Option<&[Vec<u8>]>, quiet: bool) -> ExecResult {
    let (config, lock, old) = match read_state(sh, cmd) {
        Ok(state) => state,
        Err(status) => return Ok(status),
    };
    let fetching = match update {
        None => Fetching::Missing,
        Some(names) => Fetching::Update(names.iter().map(|n| String::from_utf8_lossy(n).into_owned()).collect()),
    };
    let mut r = Resolver::new(sh, &config, old.clone(), fetching);
    r.verbose = !quiet;
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
    let (used, problems) = (r.used, r.problems);
    print_problems(sh, Some(cmd), &problems);
    if failed {
        return Ok(1);
    }
    let mut out = String::new();
    let mut moved = false;
    for (pin, label) in &used {
        match old.iter().find(|p| p.url == pin.url && p.at == pin.at) {
            None => out.push_str(&format!("Locking {label} at {}\n", short(&pin.commit))),
            Some(p) if p.commit != pin.commit => {
                moved = true;
                out.push_str(&format!(
                    "Updating {label} {}..{}\n",
                    short(&p.commit),
                    short(&pin.commit)
                ));
            }
            Some(_) => {}
        }
    }
    if update.is_some() && !moved {
        out.push_str("No updates\n");
    }
    let pins: Vec<Pin> = used.into_iter().map(|(p, _)| p).collect();
    let empty = pins.is_empty() && enabled.is_empty();
    if empty && crate::sys::stat(&lock).is_none() {
        if !quiet {
            sh.out(b"No plugins enabled and no git sources\n");
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
            let names: Vec<&str> = enabled.iter().map(|p| p.name.as_str()).collect();
            out.push_str(&format!("{} locked", count(pins.len(), "git source")));
            match names.is_empty() {
                true => out.push_str(", no plugins enabled\n"),
                false => out.push_str(&format!(
                    ", {} enabled: {}\n",
                    count(names.len(), "plugin"),
                    names.join(", ")
                )),
            }
            sh.out_status(out.as_bytes())
        }
    };
    Ok(status.max(if problems.is_empty() { 0 } else { 1 }))
}

/// `plugin check`: says which git sources have newer commits than those in
/// `plugins.lock` (asking with `git ls-remote`), and which aren't installed,
/// without fetching or changing anything. The status is 0 unless something
/// couldn't be checked.
pub fn check(sh: &mut Shell, cmd: &[u8]) -> ExecResult {
    let (config, _, pins) = match read_state(sh, cmd) {
        Ok(state) => state,
        Err(status) => return Ok(status),
    };
    let mut r = Resolver::new(sh, &config, pins, Fetching::No);
    let _ = resolve_all(&mut r, &config);
    let (used, problems, mut missing) = (r.used, r.problems, r.missing);
    print_problems(sh, Some(cmd), &problems);
    let mut status = if problems.is_empty() { 0 } else { 1 };
    let (mut checked, mut newer) = (0, 0);
    for (pin, label) in &used {
        // A commit given with `rev` has nothing newer.
        if matches!(pin.at, GitRef::Rev(_)) {
            checked += 1;
            continue;
        }
        match fetch::remote_commit(sh, &pin.url, &pin.at) {
            Ok(c) if c == pin.commit => checked += 1,
            Ok(c) => {
                checked += 1;
                newer += 1;
                sh.out(format!("Update available: {label} {}..{}\n", short(&pin.commit), short(&c)).as_bytes());
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
        let msg = format!("Not installed: {} (run plugin sync)\n", missing.join(", "));
        sh.out(msg.as_bytes());
    }
    let msg = match newer {
        0 if checked == 0 => "No git sources to check\n".to_string(),
        0 => format!("{} up to date\n", count(checked, "git source")),
        _ => format!(
            "{} of {} can be updated (run plugin update)\n",
            newer,
            count(checked, "git source")
        ),
    };
    sh.out(msg.as_bytes());
    Ok(status)
}

/// The path of the plugin that `plugin load ARG` loads from a source (`ARG`
/// a source, or `SOURCE/NAME`), if it is installed. For `plugin unload`.
pub fn location(sh: &mut Shell, arg: &[u8]) -> Option<Vec<u8>> {
    let config = read_config(sh, &mut Vec::new()).unwrap_or_default();
    let text = String::from_utf8_lossy(arg);
    let (src, name, only) = match text.split_once('/') {
        Some((src, name)) if valid_name(name) => (src, name, false),
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
/// `SOURCE/NAME` for each plugin of a collection, with the plugin's path.
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
            for p in super::available_names(&root) {
                if let Some(found) = super::find_in(&root, &p) {
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
    use super::{GitRef, Pin, lock_text, toml_str};

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
