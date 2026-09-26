//! Saving the shell's state as commands that restore it
//! (`__luish_internal savestate`), and the difference between two states
//! (for the startup cache, `startcache.rs`).
//!
//! The state is the working directory and the directory stack, the file
//! mode mask, variables (with their export and readonly attributes), traps,
//! functions, aliases, loaded plugins and options. It is written as shell
//! commands, so it is restored
//! by running them with `.`. Restoring sets everything that was saved, but
//! doesn't remove what wasn't (such as variables set since).

use crate::builtins::single_quote;
use crate::options::{OPTIONS, Opt};
use crate::shell::Shell;
use crate::{signals, sys, unparse};

/// Options that describe how the shell was started or runs, rather than
/// settings to restore.
const MODE_OPTIONS: &[Opt] = &[Opt::Interactive, Opt::Stdin, Opt::Monitor, Opt::Noexec];

/// Variables that belong to the process.
const PROCESS_VARS: &[&[u8]] = &[b"PPID", b"LINENO"];

/// The kinds of state, in the order their commands must run: the directory
/// comes first (so that the saved `PWD` and `OLDPWD` win), functions come
/// before aliases (which would otherwise be expanded in their bodies;
/// command names that are aliases are also quoted, for a shell that has
/// them already), and options come last (so that `set -e`, `-u`, `-x` or
/// `-a` don't affect the rest). Plugins are loaded after everything that
/// their top level might use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Dir,
    DirStack,
    Umask,
    Var,
    Readonly,
    Trap,
    Function,
    Alias,
    Plugin,
    Option,
}

/// One piece of state: its kind and name, and the commands that restore it.
pub struct Entry {
    pub kind: Kind,
    pub name: Vec<u8>,
    pub text: Vec<u8>,
}

impl Shell {
    /// The shell's state, as commands.
    pub fn dump_state(&self) -> Vec<u8> {
        self.state_entries().into_iter().flat_map(|e| e.text).collect()
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
            if var.exported {
                t.extend_from_slice(b"export ");
            }
            t.extend_from_slice(name);
            if let Some(v) = &var.value {
                t.push(b'=');
                t.extend(single_quote(v));
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

        let mut funcs: Vec<_> = self.functions.iter().collect();
        funcs.sort_by(|a, b| a.0.cmp(b.0));
        for (name, body) in funcs {
            // The name of a function can't be quoted to keep it from being
            // expanded as an alias (restored later).
            let mut t = Vec::new();
            if self.aliases.contains_key(name) {
                t.extend_from_slice(b"command unalias ");
                t.extend(single_quote(name));
                t.extend_from_slice(b" 2>/dev/null\n");
            }
            t.extend(unparse::function(name, body, &self.aliases));
            add(Kind::Function, name, t);
        }

        let mut aliases: Vec<_> = self.aliases.iter().collect();
        aliases.sort();
        for (name, value) in aliases {
            let mut t = b"command alias ".to_vec();
            t.extend(single_quote(name));
            t.push(b'=');
            t.extend(single_quote(value));
            t.push(b'\n');
            add(Kind::Alias, name, t);
        }

        if let Some(host) = &self.plugins {
            for (name, path) in host.loaded() {
                let mut t = b"__luish_internal plugin load ".to_vec();
                t.extend(single_quote(&path));
                t.push(b'\n');
                add(Kind::Plugin, &name, t);
            }
        }

        for (o, _, name) in OPTIONS {
            if !MODE_OPTIONS.contains(o) {
                let sign = if self.options.get(*o) { '-' } else { '+' };
                add(
                    Kind::Option,
                    name.as_bytes(),
                    format!("set {sign}o {name}\n").into_bytes(),
                );
            }
        }
        out
    }
}

/// Commands that turn state `before` into state `after`: removals first,
/// then everything new or changed, in order. A read-only variable can't be
/// removed, so it is left alone.
pub fn difference(before: &[Entry], after: &[Entry]) -> Vec<u8> {
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
        match e.kind {
            Kind::Var => out.extend([b"unset -v ".to_vec(), e.name.clone()].concat()),
            Kind::Function => out.extend([b"unset -f ".to_vec(), e.name.clone()].concat()),
            Kind::Alias => out.extend([b"command unalias ".to_vec(), quoted()].concat()),
            Kind::Trap => out.extend([b"trap - ".to_vec(), e.name.clone()].concat()),
            Kind::Plugin => out.extend([b"__luish_internal plugin unload ".to_vec(), quoted()].concat()),
            Kind::DirStack => out.extend_from_slice(b"command dirs -c"),
            Kind::Dir | Kind::Umask | Kind::Readonly | Kind::Option => continue,
        }
        out.push(b'\n');
    }
    for e in after {
        match old.get(&(e.kind, e.name.clone())) {
            Some(&i) if before[i].text == e.text => {}
            _ => out.extend_from_slice(&e.text),
        }
    }
    out
}
