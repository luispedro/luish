//! `plugin add [-y] SPEC [NAME]` (DEVELOPING.md): adds a plugin to
//! `config.toml`, after asking, then runs `plugin sync` and loads it.
//!
//! `SPEC` is flexible: a GitHub repository (`OWNER/REPO`, `gh:OWNER/REPO`,
//! or a URL, with `/tree/REF/SUBDIR` as GitHub shows a directory), another
//! git URL, a local path (also as a `file://` URL, which is a git source if
//! it is a repository), or a plugin of a source that `config.toml` names
//! (`SOURCE/NAME`, such as `std/NAME`, or `NAME`). A git source is fetched
//! into a temporary directory first, to see what it holds. A source goes to
//! `plugins.enabled` as `NAME = { gh = ... }`, except a collection of more
//! than one plugin, which goes to `plugins.available`. The file is edited
//! as text, so that its comments and layout stay, and read again to check
//! the result.

use super::fetch;
use super::package::{self, GitRef, toml_str, valid_name};
use crate::interactive::to_path;
use crate::shell::{ExecResult, Shell};
use crate::signals;
use crate::sys;

/// What a `SPEC` names.
#[derive(Debug, PartialEq)]
enum Spec {
    /// A plugin of a named source (`SOURCE/NAME`), or `NAME`: a source
    /// that is one plugin, or a plugin in the plugin directory.
    Named(Option<String>, String),
    /// A git source: its `gh` or `git` field, the ref and subdirectory,
    /// and the name it suggests.
    Git {
        field: &'static str,
        value: String,
        at: Option<(&'static str, String)>,
        subdir: Option<String>,
        name: String,
    },
    /// A local path, as written.
    Path(String),
}

/// Whether `s` looks like `OWNER/REPO` on GitHub.
fn is_gh_repo(s: &str) -> bool {
    s.split_once('/').is_some_and(|(owner, repo)| {
        let ok = |s: &str| {
            !s.is_empty()
                && !s.starts_with(['.', '-'])
                && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        ok(owner) && ok(repo)
    })
}

/// A GitHub repository from what follows `github.com/` (or `gh:`):
/// `OWNER/REPO[.git][/tree/REF[/SUBDIR]]`.
fn github(rest: &str) -> Result<Spec, String> {
    let rest = rest.trim_end_matches('/');
    let mut parts = rest.splitn(3, '/');
    let (owner, repo) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    let value = format!("{owner}/{repo}");
    if !is_gh_repo(&value) {
        return Err(format!("{rest}: not a GitHub repository (OWNER/REPO)"));
    }
    let (mut at, mut subdir) = (None, None);
    if let Some(tail) = parts.next() {
        let Some(tail) = tail.strip_prefix("tree/") else {
            return Err(format!("{rest}: not a GitHub repository or directory"));
        };
        let (r, dir) = tail.split_once('/').unwrap_or((tail, ""));
        let r = r.to_string();
        at = Some(match super::fetch::is_hash(&r) {
            true => ("rev", r),
            false => ("branch", r),
        });
        subdir = (!dir.is_empty()).then(|| dir.to_string());
    }
    let name = subdir
        .as_deref()
        .and_then(|s| s.rsplit('/').next())
        .unwrap_or(repo)
        .to_string();
    Ok(Spec::Git {
        field: "gh",
        value,
        at,
        subdir,
        name,
    })
}

/// Decodes the `%XX` escapes of a URL's path.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (b[i], hex) {
            (b'%', Some(c)) => {
                out.push(c);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reads `SPEC`. `named` says whether a name is a source in
/// `config.toml`, `exists` whether a relative path exists, and `is_repo`
/// whether an absolute path is a git repository.
fn parse(
    spec: &str,
    named: impl Fn(&str) -> bool,
    exists: impl Fn(&str) -> bool,
    is_repo: impl Fn(&str) -> bool,
) -> Result<Spec, String> {
    if spec.is_empty() {
        return Err("empty plugin".into());
    }
    // `file:///PATH`, `file://localhost/PATH` or `file:/PATH`.
    if let Some(rest) = spec.strip_prefix("file:") {
        let rest = match rest.strip_prefix("//") {
            Some(r) => r.strip_prefix("localhost").unwrap_or(r),
            None => rest,
        };
        if !rest.starts_with('/') {
            return Err(format!("{spec}: not a file URL (file:///PATH)"));
        }
        let path = percent_decode(rest);
        if !is_repo(&path) {
            return Ok(Spec::Path(path));
        }
        let last = rest.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        return Ok(Spec::Git {
            field: "git",
            value: format!("file://{rest}"),
            at: None,
            subdir: None,
            name: percent_decode(last.strip_suffix(".git").unwrap_or(last)),
        });
    }
    if spec.starts_with(['/', '~']) || spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../")
    {
        return Ok(Spec::Path(spec.into()));
    }
    for prefix in ["gh:", "github:"] {
        if let Some(rest) = spec.strip_prefix(prefix) {
            return github(rest);
        }
    }
    let bare = ["https://", "http://"]
        .iter()
        .find_map(|p| spec.strip_prefix(p))
        .unwrap_or(spec);
    let bare = bare.strip_prefix("www.").unwrap_or(bare);
    if let Some(rest) = bare.strip_prefix("github.com/") {
        return github(rest);
    }
    let url = spec.contains("://") || spec.split_once(':').is_some_and(|(host, _)| host.contains('@'));
    if url {
        let path = spec.trim_end_matches('/');
        let last = path.rsplit(['/', ':']).next().unwrap_or("");
        let name = last.strip_suffix(".git").unwrap_or(last).to_string();
        return Ok(Spec::Git {
            field: "git",
            value: spec.into(),
            at: None,
            subdir: None,
            name,
        });
    }
    match spec.split_once('/') {
        Some((src, name)) if !name.contains('/') && named(src) => {
            return Ok(Spec::Named(Some(src.into()), name.into()));
        }
        None if named(spec) => return Ok(Spec::Named(None, spec.into())),
        _ => {}
    }
    if exists(spec) {
        return Ok(Spec::Path(spec.into()));
    }
    if is_gh_repo(spec) {
        return github(spec);
    }
    Err(format!(
        "{spec}: no such file or directory, and not a GitHub repository (OWNER/REPO) or URL"
    ))
}

/// A TOML key: bare if it can be.
fn toml_key(s: &str) -> String {
    match !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)) {
        true => s.to_string(),
        false => toml_str(s),
    }
}

/// The inline table of a source: `{ gh = "OWNER/REPO", branch = "main" }`.
fn table(fields: &[(&str, &str)]) -> String {
    let fields: Vec<String> = fields.iter().map(|(k, v)| format!("{k} = {}", toml_str(v))).collect();
    format!("{{ {} }}", fields.join(", "))
}

/// Whether `line` is the header of the table `name` (such as
/// `[plugins.enabled]`).
fn is_header(line: &str, name: &str) -> bool {
    let line = line.split('#').next().unwrap_or("");
    let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    squeezed == format!("[{name}]")
}

/// `text` with `line` added to the table `name`: after its last line that
/// isn't blank, or in a new table at the end.
fn insert(text: &str, name: &str, line: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    if let Some(h) = lines.iter().position(|l| is_header(l, name)) {
        let end = (h + 1..lines.len())
            .find(|&i| lines[i].trim_start().starts_with('['))
            .unwrap_or(lines.len());
        let at = (h + 1..end)
            .rev()
            .find(|&i| !lines[i].trim().is_empty())
            .map_or(h + 1, |i| i + 1);
        let mut out: String = lines[..at].concat();
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
        out.push_str(&lines[at..].concat());
        return out;
    }
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("[{name}]\n{line}\n"));
    out
}

/// Whether `path` is a git repository (with a work tree, or bare).
fn is_repo(path: &[u8]) -> bool {
    let has = |f: &[u8]| sys::stat(&[path, f].concat()).is_some();
    has(b"/.git") || (has(b"/HEAD") && super::is_dir(&[path, b"/objects"].concat()))
}

/// What a source holds.
enum Holds {
    Plugin,
    /// A collection of more than one plugin, with their names.
    Collection(Vec<String>),
}

/// What the files `root` of the source `label` hold.
fn holds(root: &[u8], label: &str) -> Result<Holds, String> {
    if !super::is_dir(root) || super::is_plugin_dir(root) {
        return Ok(Holds::Plugin);
    }
    let names: Vec<String> = (super::available_names(root).iter())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect();
    match names.len() {
        0 => Err(format!("{label}: no plugins in it")),
        1 => Ok(Holds::Plugin),
        _ => Ok(Holds::Collection(names)),
    }
}

/// Fetches a git source into a temporary directory, to see what it holds.
fn fetch_holds(sh: &mut Shell, url: &str, at: &GitRef, subdir: Option<&str>, label: &str) -> Result<Holds, String> {
    let data = fetch::data_dir(sh).ok_or("no directory for plugins (HOME is not set)")?;
    sh.out(format!("Fetching {label}\n").as_bytes());
    let commit = fetch::resolve(sh, url, at).map_err(|e| format!("{label}: {e}"))?;
    let tmp = [&data[..], format!("/.add.{}", sys::getpid()).as_bytes()].concat();
    let r = fetch::extract(sh, &tmp, url, at, &commit)
        .map_err(|e| format!("{label}: {e}"))
        .and_then(|_| {
            let mut root = fetch::checkout(&tmp, url, &commit);
            if let Some(s) = subdir {
                root.extend_from_slice(b"/");
                root.extend_from_slice(s.as_bytes());
                if sys::stat(&root).is_none() {
                    return Err(format!("{label}: no directory {s} in it"));
                }
            }
            holds(&root, label)
        });
    let _ = std::fs::remove_dir_all(to_path(&tmp));
    r
}

/// Whether the plugin directory has the plugin `name`.
fn in_plugin_dir(sh: &Shell, name: &str) -> bool {
    super::plugin_dir(sh).is_some_and(|dir| super::find_in(&dir, name.as_bytes()).is_some())
}

/// Asks on stderr, and reads the answer from standard input.
fn confirm(question: &str) -> bool {
    sys::write_all(2, question.as_bytes());
    let mut line = Vec::new();
    let mut buf = [0u8; 1];
    loop {
        match sys::read(0, &mut buf, true) {
            Ok(1) if buf[0] == b'\n' => break,
            Ok(1) => line.push(buf[0]),
            Err(libc::EINTR) if !signals::is_pending(libc::SIGINT) => {}
            _ => {
                sys::write_all(2, b"\n");
                return false;
            }
        }
    }
    matches!(line.trim_ascii().to_ascii_lowercase().as_slice(), b"y" | b"yes")
}

const USAGE: &str = "usage: plugin add [-y] PLUGIN [NAME]";

/// `plugin add [-y] SPEC [NAME]`. `cmd` is the command, for messages.
pub fn add(sh: &mut Shell, cmd: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let yes = args
        .iter()
        .take_while(|a| matches!(a.as_slice(), b"-y" | b"--yes"))
        .count();
    let (spec, given) = match &args[yes..] {
        [spec] => (spec, None),
        [spec, name] => (spec, Some(String::from_utf8_lossy(name).into_owned())),
        _ => {
            sh.berr(cmd, USAGE);
            return Ok(2);
        }
    };
    if sh.no_plugins {
        return Ok(0);
    }
    let fail = |sh: &Shell, msg: String| {
        sh.berr(cmd, msg);
        Ok(1)
    };
    let text = String::from_utf8_lossy(spec).into_owned();
    let expand = |sh: &Shell, p: &str| super::absolute(sh, &crate::config::tilde(sh, p.as_bytes()));
    let parsed = parse(
        &text,
        |n| package::is_named_source(sh, n) || (n == text && in_plugin_dir(sh, n)),
        |p| sys::stat(&expand(sh, p)).is_some(),
        |p| is_repo(p.as_bytes()),
    );
    let spec = match parsed {
        Ok(s) => s,
        Err(e) => return fail(sh, e),
    };
    // The key and value of the line to add, and what the source holds.
    let (name, value, holds) = match spec {
        Spec::Named(src, name) => {
            let key = match &src {
                Some(src) => toml_str(&format!("{src}/{name}")),
                None => toml_key(&name),
            };
            if given.is_some() {
                return fail(sh, format!("{text}: a plugin of a source keeps its name"));
            }
            (name, format!("{key} = \"*\""), Holds::Plugin)
        }
        Spec::Git {
            field,
            value,
            at,
            subdir,
            name,
        } => {
            let mut fields = vec![(field, value.as_str())];
            if let Some((k, v)) = &at {
                fields.push((k, v));
            }
            if let Some(s) = &subdir {
                fields.push(("subdir", s));
            }
            let (url, label) = match field {
                "gh" => (package::github_url(&value), value.clone()),
                _ => (value.clone(), value.clone()),
            };
            let at = match &at {
                None => GitRef::Head,
                Some(("rev", r)) => GitRef::Rev(r.clone()),
                Some((_, b)) => GitRef::Branch(b.clone()),
            };
            let holds = match fetch_holds(sh, &url, &at, subdir.as_deref(), &label) {
                Ok(h) => h,
                Err(e) => return fail(sh, e),
            };
            let name = given.unwrap_or(name);
            let line = format!("{} = {}", toml_key(&name), table(&fields));
            (name, line, holds)
        }
        Spec::Path(p) => {
            let abs = expand(sh, &p);
            if sys::stat(&abs).is_none() {
                return fail(sh, format!("{p}: no such file or directory"));
            }
            let written = match p.starts_with('~') {
                true => p.trim_end_matches('/').to_string(),
                false => String::from_utf8_lossy(&abs).into_owned(),
            };
            let holds = match holds(&abs, &p) {
                Ok(h) => h,
                Err(e) => return fail(sh, e),
            };
            let base = String::from_utf8_lossy(&super::plugin_name(&abs, super::at_path(&abs).kind)).into_owned();
            let name = given.unwrap_or(base);
            let line = format!("{} = {}", toml_key(&name), table(&[("path", &written)]));
            (name, line, holds)
        }
    };
    let table_name = match holds {
        Holds::Plugin => "plugins.enabled",
        Holds::Collection(_) => "plugins.available",
    };
    if !valid_name(&name) {
        return fail(
            sh,
            format!("{name:?}: bad plugin name (give one as the second argument)"),
        );
    }
    let Some(file) = crate::config::path(sh) else {
        return fail(sh, "no configuration directory (HOME is not set)".into());
    };
    // Edit the file a symbolic link points to, not the link.
    let file = match std::fs::canonicalize(crate::interactive::to_path(&file)) {
        Ok(p) => std::os::unix::ffi::OsStrExt::as_bytes(p.as_os_str()).to_vec(),
        Err(_) => file,
    };
    let old = std::fs::read(crate::interactive::to_path(&file)).unwrap_or_default();
    let (enabled, available) = match package::config_names(sh, &file, &old) {
        Ok(names) => names,
        Err(e) => return fail(sh, e),
    };
    let taken = match table_name {
        "plugins.enabled" => enabled.contains(&name),
        _ => available.contains(&name) || name == "std",
    };
    if taken {
        return fail(
            sh,
            format!("{name}: already in {table_name} (give another name as the second argument)"),
        );
    }
    let Ok(old) = String::from_utf8(old) else {
        return fail(sh, format!("{}: not valid UTF-8", String::from_utf8_lossy(&file)));
    };
    let new = insert(&old, table_name, &value);
    let added = match package::config_names(sh, &file, new.as_bytes()) {
        Ok((enabled, available)) => match table_name {
            "plugins.enabled" => enabled.contains(&name),
            _ => available.contains(&name),
        },
        Err(_) => false,
    };
    let shown = String::from_utf8_lossy(&file);
    if !added {
        return fail(
            sh,
            format!("cannot add to {shown}: add this to its [{table_name}] table yourself:\n{value}"),
        );
    }
    let what = match &holds {
        Holds::Plugin => String::new(),
        Holds::Collection(names) => format!(" (a collection of {})", names.join(", ")),
    };
    let question =
        format!("Adding to [{table_name}] in {shown}{what}:\n    {value}\nand running plugin sync. Continue? [y/N] ");
    if yes == 0 && !confirm(&question) {
        sys::write_all(2, b"Nothing changed\n");
        return Ok(1);
    }
    if let Err(e) = package::write_file(&file, &new) {
        return fail(sh, e);
    }
    let status = super::sync(sh, cmd, None, false)?;
    match holds {
        _ if status != 0 => Ok(status),
        Holds::Plugin => package::load_added(sh, cmd, &name),
        Holds::Collection(names) => {
            let msg = format!(
                "{name} has {} plugins: {}. Load one with plugin load {name}/{2}, or enable it with plugin add {name}/{2}\n",
                names.len(),
                names.join(", "),
                names[0]
            );
            Ok(sh.out_status(msg.as_bytes()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Spec, String> {
        parse(s, |n| n == "std", |p| p == "here/dir", |p| p.starts_with("/repo"))
    }

    fn gh(value: &str, at: Option<(&'static str, &str)>, subdir: Option<&str>, name: &str) -> Spec {
        Spec::Git {
            field: "gh",
            value: value.into(),
            at: at.map(|(k, v)| (k, v.into())),
            subdir: subdir.map(Into::into),
            name: name.into(),
        }
    }

    #[test]
    fn specs() {
        let repo = gh("owner/repo", None, None, "repo");
        assert_eq!(p("owner/repo"), Ok(repo));
        for s in [
            "gh:owner/repo",
            "github.com/owner/repo",
            "https://github.com/owner/repo",
            "https://www.github.com/owner/repo.git",
            "http://github.com/owner/repo/",
        ] {
            assert_eq!(p(s), Ok(gh("owner/repo", None, None, "repo")), "{s}");
        }
        assert_eq!(
            p("https://github.com/o/r/tree/dev/plugins/x"),
            Ok(gh("o/r", Some(("branch", "dev")), Some("plugins/x"), "x"))
        );
        let hash = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            p(&format!("github.com/o/r/tree/{hash}")),
            Ok(gh("o/r", Some(("rev", hash)), None, "r"))
        );
        assert!(p("https://github.com/o/r/blob/main/x.rhai").is_err());
        assert_eq!(
            p("git@example.org:me/thing.git"),
            Ok(Spec::Git {
                field: "git",
                value: "git@example.org:me/thing.git".into(),
                at: None,
                subdir: None,
                name: "thing".into()
            })
        );
        assert!(matches!(p("ssh://host/srv/x.git"), Ok(Spec::Git { field: "git", name, .. }) if name == "x"));
        assert!(
            matches!(p("file:///repo/a%20b.git/"), Ok(Spec::Git { field: "git", name, value, .. })
                if name == "a b" && value == "file:///repo/a%20b.git/")
        );
        for s in ["file:///x/a%20b", "file://localhost/x/a%20b", "file:/x/a%20b"] {
            assert_eq!(p(s), Ok(Spec::Path("/x/a b".into())), "{s}");
        }
        assert!(p("file://host/x").is_err());
        assert_eq!(p("std/hello"), Ok(Spec::Named(Some("std".into()), "hello".into())));
        assert_eq!(p("std"), Ok(Spec::Named(None, "std".into())));
        for s in ["./x", "../x", "/x", "~/x", "here/dir", "."] {
            assert_eq!(p(s), Ok(Spec::Path(s.into())), "{s}");
        }
        assert!(p("nothing").is_err());
        assert!(p("a/b/c").is_err());
    }

    #[test]
    fn inserting() {
        let line = "x = { gh = \"o/r\" }";
        assert_eq!(
            insert("", "plugins.enabled", line),
            "[plugins.enabled]\nx = { gh = \"o/r\" }\n"
        );
        assert_eq!(
            insert("a = 1", "plugins.enabled", line),
            "a = 1\n\n[plugins.enabled]\nx = { gh = \"o/r\" }\n"
        );
        let text = "[plugins.enabled] # mine\n\"std/a\" = \"*\"\n# b = \"*\"\n\n[options]\nx = 1\n";
        assert_eq!(
            insert(text, "plugins.enabled", line),
            "[plugins.enabled] # mine\n\"std/a\" = \"*\"\n# b = \"*\"\nx = { gh = \"o/r\" }\n\n[options]\nx = 1\n"
        );
        assert_eq!(
            insert("[ plugins.enabled ]", "plugins.enabled", line),
            "[ plugins.enabled ]\nx = { gh = \"o/r\" }\n"
        );
    }
}
