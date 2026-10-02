//! Saving the shell's state as commands that restore it
//! (`__luish_internal savestate`), and the difference between two states
//! (for the startup cache, `startcache.rs`).
//!
//! The state is the working directory and the directory stack, the file
//! mode mask, variables (with their export and readonly attributes), traps,
//! functions, aliases, loaded plugins, key bindings and options. It is written as shell
//! commands, so it is restored
//! by running them with `.`. Restoring sets everything that was saved, but
//! doesn't remove what wasn't (such as variables set since).

use crate::builtins::misc::alias_command;
use crate::builtins::{quote_value, single_quote};
use crate::lexer::AliasKind;
use crate::options::{EXTENDED, OPTIONS, Opt};
use crate::shell::Shell;
use crate::{signals, sys, unparse};

/// Options that describe how the shell was started or runs, rather than
/// settings to restore.
const MODE_OPTIONS: &[Opt] = &[Opt::Interactive, Opt::Stdin, Opt::Monitor, Opt::Noexec];

/// Options that change how functions are parsed: restored before them.
const SYNTAX_OPTIONS: &[Opt] = &[Opt::Bareglobqual];

/// Variables that belong to the process.
const PROCESS_VARS: &[&[u8]] = &[b"PPID", b"LINENO", b"SHLVL"];

/// The kinds of state, in the order their commands must run: the directory
/// comes first (so that the saved `PWD` and `OLDPWD` win), functions come
/// before aliases (which would otherwise be expanded in their bodies;
/// words that are aliases are also quoted, for a shell that has them
/// already), and options come last (so that `set -e`, `-u`, `-x` or `-a`
/// don't affect the rest), except those that change how function bodies
/// are parsed, which come just before the functions. Plugins are loaded
/// after everything that their top level might use. The commands from the
/// aliases on are grouped in braces (see [`join`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Dir,
    DirStack,
    Umask,
    Var,
    Readonly,
    Trap,
    SyntaxOption,
    Function,
    Alias,
    SuffixAlias,
    Plugin,
    Binding,
    Option,
}

/// Every kind, in order (for [`Kind::code`]).
const KINDS: [Kind; 13] = [
    Kind::Dir,
    Kind::DirStack,
    Kind::Umask,
    Kind::Var,
    Kind::Readonly,
    Kind::Trap,
    Kind::SyntaxOption,
    Kind::Function,
    Kind::Alias,
    Kind::SuffixAlias,
    Kind::Plugin,
    Kind::Binding,
    Kind::Option,
];

impl Kind {
    /// A letter for the kind, in the startup cache.
    pub fn code(self) -> u8 {
        b'a' + self as u8
    }

    pub fn from_code(c: u8) -> Option<Kind> {
        KINDS.get(c.wrapping_sub(b'a') as usize).copied()
    }

    /// What the kind is called in reports (`__luish_internal check-cache`).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Dir => "directory",
            Kind::DirStack => "directory stack",
            Kind::Umask => "umask",
            Kind::Var => "variable",
            Kind::Readonly => "readonly",
            Kind::Trap => "trap",
            Kind::SyntaxOption | Kind::Option => "option",
            Kind::Function => "function",
            Kind::Alias => "alias",
            Kind::SuffixAlias => "suffix alias",
            Kind::Plugin => "plugin",
            Kind::Binding => "key binding",
        }
    }
}

/// Line numbers for `__luish_internal function-file`: each as the
/// difference from the one before, separated by commas (`12,1,0,3`).
pub fn encode_lines(lines: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev = 0;
    for &l in lines {
        if !out.is_empty() {
            out.push(b',');
        }
        out.extend_from_slice((i64::from(l) - prev).to_string().as_bytes());
        prev = i64::from(l);
    }
    out
}

/// The line numbers of [`encode_lines`], or `None` if `text` isn't such a
/// list.
pub fn decode_lines(text: &[u8]) -> Option<Vec<u32>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut prev = 0i64;
    text.split(|&c| c == b',')
        .map(|d| {
            prev += std::str::from_utf8(d).ok()?.parse::<i64>().ok()?;
            u32::try_from(prev).ok()
        })
        .collect()
}

/// One piece of state: its kind and name, and the commands that restore it.
pub struct Entry {
    pub kind: Kind,
    pub name: Vec<u8>,
    pub text: Vec<u8>,
    /// A variable assigned since [`crate::vars::Vars::clear_assigned`]:
    /// part of a [`difference`] even if its value is the same, so that a
    /// cache replayed where it was different (or unset) sets it.
    pub assigned: bool,
}

impl Shell {
    /// The shell's state, as commands.
    pub fn dump_state(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let entries = self.state_entries();
        join(
            &mut out,
            &entries.iter().map(|e| (e.kind, &e.text[..])).collect::<Vec<_>>(),
        );
        out
    }

    /// The shell's state, one entry per variable, function, and so on, in
    /// the order their commands must run.
    pub fn state_entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let mut add = |kind, name: &[u8], text: Vec<u8>| {
            out.push(Entry {
                kind,
                name: name.to_vec(),
                text,
                assigned: false,
            })
        };
        // `command`, in case a function of the same name is defined.
        if let Some(dir) = &self.curdir {
            let mut t = b"command cd -- ".to_vec();
            t.extend(single_quote(dir));
            t.push(b'\n');
            add(Kind::Dir, b"", t);
        }
        if !self.dirstack.is_empty() {
            let mut t = b"command dirs --".to_vec();
            for d in &self.dirstack {
                t.push(b' ');
                t.extend(single_quote(d));
            }
            t.push(b'\n');
            add(Kind::DirStack, b"", t);
        }
        let mask = sys::umask(0);
        sys::umask(mask);
        add(Kind::Umask, b"", format!("command umask {mask:04o}\n").into_bytes());

        let vars = self.vars.sorted();
        for (name, var) in &vars {
            if PROCESS_VARS.contains(&name.as_slice()) || (!var.exported && var.value.is_none()) {
                continue;
            }
            let mut t = Vec::new();
            if let Some(crate::vars::Value::Assoc(_)) = var.value {
                t.extend_from_slice(b"typeset -gA ");
                t.extend_from_slice(name);
                t.push(b'\n');
            }
            if var.exported {
                t.extend_from_slice(b"export ");
            }
            t.extend_from_slice(name);
            if let Some(v) = &var.value {
                t.push(b'=');
                t.extend(quote_value(v));
            }
            t.push(b'\n');
            add(Kind::Var, name, t);
        }
        for (name, var) in &vars {
            if var.readonly && !PROCESS_VARS.contains(&name.as_slice()) {
                add(Kind::Readonly, name, [b"readonly ", name.as_slice(), b"\n"].concat());
            }
        }

        for (sig, t) in self.traps.iter().enumerate() {
            if let Some(action) = t {
                let name = signals::name(sig as i32);
                let mut t = b"trap -- ".to_vec();
                t.extend(single_quote(action));
                t.push(b' ');
                t.extend_from_slice(name.as_bytes());
                t.push(b'\n');
                add(Kind::Trap, name.as_bytes(), t);
            }
        }

        let option = |o: &Opt, name: &str| {
            let sign = if self.options.get(*o) { '-' } else { '+' };
            format!("set {sign}o {name}\n").into_bytes()
        };
        let extended = |o: &Opt, name: &str| {
            let cmd = if self.options.get(*o) { "setopt" } else { "unsetopt" };
            format!("{cmd} {name}\n").into_bytes()
        };
        for (o, name) in EXTENDED {
            if SYNTAX_OPTIONS.contains(o) {
                add(Kind::SyntaxOption, name.as_bytes(), extended(o, name));
            }
        }

        let mut funcs: Vec<_> = self.functions.iter().collect();
        funcs.sort_by(|a, b| a.0.cmp(b.0));
        for (name, func) in funcs {
            let body = &func.body;
            // The name of a function can't be quoted to keep it from being
            // expanded as an alias (restored later).
            let mut t = Vec::new();
            if self.aliases.contains(name) {
                t.extend_from_slice(b"command unalias -- ");
                t.extend(single_quote(name));
                t.extend_from_slice(b" 2>/dev/null\n");
            }
            let (text, globqual) = unparse::function(name, body, &self.aliases);
            // A function with a glob qualifier, defined before the option
            // was turned off, needs it to be read back.
            let wrap = globqual && !self.opt(Opt::Bareglobqual);
            if wrap {
                t.extend_from_slice(b"setopt glob.bare_qualifiers\n");
            }
            t.extend(text);
            if wrap {
                t.extend_from_slice(b"unsetopt glob.bare_qualifiers\n");
            }
            if let Some(file) = &func.file {
                t.extend_from_slice(b"command __luish_internal function-file ");
                t.extend(single_quote(name));
                t.push(b' ');
                t.extend(single_quote(&file.name));
                // The lines of its body in the file, which the text above
                // doesn't keep, and where a relative file was.
                if func.lines_in_file {
                    t.push(b' ');
                    match &func.pending_lines {
                        Some(lines) => t.extend_from_slice(lines),
                        None => t.extend(encode_lines(&unparse::body_lines(body))),
                    }
                    if let Some(dir) = file.dir() {
                        t.push(b' ');
                        t.extend(single_quote(dir));
                    }
                }
                t.push(b'\n');
            }
            add(Kind::Function, name, t);
        }

        for (name, a) in self.aliases.sorted() {
            add(
                Kind::Alias,
                name,
                [b"command ", &alias_command(name, &a.value, a.kind())[..]].concat(),
            );
        }
        for (suffix, value) in self.aliases.sorted_suffixes() {
            let t = [b"command ", &alias_command(suffix, value, AliasKind::Suffix)[..]].concat();
            add(Kind::SuffixAlias, suffix, t);
        }

        if let Some(host) = &self.plugins {
            for (name, path) in host.loaded() {
                // `restore`, not `load`: what a directory plugin's `rc.lsh`
                // did is in the state already.
                let mut t = b"__luish_internal plugin restore ".to_vec();
                t.extend(single_quote(&name));
                t.push(b' ');
                t.extend(single_quote(&path));
                t.push(b'\n');
                add(Kind::Plugin, &name, t);
            }
        }

        for (seq, t) in self.keymap.state() {
            add(Kind::Binding, &seq, t);
        }

        for (o, _, name) in OPTIONS {
            if !MODE_OPTIONS.contains(o) {
                add(Kind::Option, name.as_bytes(), option(o, name));
            }
        }
        for (o, name) in EXTENDED {
            if !SYNTAX_OPTIONS.contains(o) {
                add(Kind::Option, name.as_bytes(), extended(o, name));
            }
        }
        // `PWD` is the directory's (`cd` there and back is no change).
        for e in out.iter_mut().filter(|e| e.kind == Kind::Var && e.name != b"PWD") {
            e.assigned = self.vars.var(&e.name).is_some_and(|v| v.assigned);
        }
        out
    }
}

/// One difference between two states: an entry that is new or changed (or
/// assigned), with the commands that restore it, or one that was removed,
/// with the commands that remove it.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub kind: Kind,
    pub name: Vec<u8>,
    pub text: Vec<u8>,
    pub removed: bool,
}

/// What turns state `before` into state `after`: removals first, then
/// everything new or changed (or assigned), in order. A read-only variable
/// can't be removed, so it is left alone.
pub fn changes(before: &[Entry], after: &[Entry]) -> Vec<Change> {
    use std::collections::HashMap;
    let index = |entries: &[Entry]| -> HashMap<(Kind, Vec<u8>), usize> {
        entries
            .iter()
            .enumerate()
            .map(|(i, e)| ((e.kind, e.name.clone()), i))
            .collect()
    };
    let (old, new) = (index(before), index(after));
    let mut out = Vec::new();
    for e in before {
        if new.contains_key(&(e.kind, e.name.clone())) {
            continue;
        }
        let quoted = || single_quote(&e.name);
        let mut text = match e.kind {
            Kind::Var => [b"unset -v ".to_vec(), e.name.clone()].concat(),
            Kind::Function => [b"unset -f ".to_vec(), e.name.clone()].concat(),
            Kind::Alias => [b"command unalias -- ".to_vec(), quoted()].concat(),
            Kind::SuffixAlias => [b"command unalias -s -- ".to_vec(), quoted()].concat(),
            Kind::Trap => [b"trap - ".to_vec(), e.name.clone()].concat(),
            Kind::Plugin => [b"__luish_internal plugin unload ".to_vec(), quoted()].concat(),
            Kind::DirStack => b"command dirs -c".to_vec(),
            Kind::Binding => crate::interactive::keys::restore_command(&e.name),
            Kind::Dir | Kind::Umask | Kind::Readonly | Kind::SyntaxOption | Kind::Option => continue,
        };
        if e.kind != Kind::Binding {
            text.push(b'\n');
        }
        out.push(Change {
            kind: e.kind,
            name: e.name.clone(),
            text,
            removed: true,
        });
    }
    for e in after {
        let same = old
            .get(&(e.kind, e.name.clone()))
            .is_some_and(|&i| before[i].text == e.text);
        if e.assigned || !same {
            out.push(Change {
                kind: e.kind,
                name: e.name.clone(),
                text: e.text.clone(),
                removed: false,
            });
        }
    }
    out
}

/// Appends the commands of `entries`. If aliases are among them, those
/// from the aliases on are grouped in braces: the group is parsed before
/// any of it runs, so that the aliases don't change the commands after
/// them (a global alias could be any word).
pub fn join(out: &mut Vec<u8>, entries: &[(Kind, &[u8])]) {
    let has_aliases = entries.iter().any(|e| matches!(e.0, Kind::Alias | Kind::SuffixAlias));
    let group = entries.iter().position(|e| e.0 >= Kind::Alias).filter(|_| has_aliases);
    for (i, e) in entries.iter().enumerate() {
        if group == Some(i) {
            out.extend_from_slice(b"{\n");
        }
        out.extend_from_slice(e.1);
    }
    if group.is_some() {
        out.extend_from_slice(b"}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let lines = [12, 13, 13, 9, 0, 4_000_000_000];
        let text = encode_lines(&lines);
        assert_eq!(text, b"12,1,0,-4,-9,4000000000");
        assert_eq!(decode_lines(&text).as_deref(), Some(&lines[..]));
        assert_eq!(decode_lines(b"").as_deref(), Some(&[][..]));
        for bad in [&b"1,"[..], b"x", b"1,-2", b"5000000000", b",1"] {
            assert_eq!(decode_lines(bad), None, "{}", String::from_utf8_lossy(bad));
        }
    }
}
