//! Saving the shell's state as commands that restore it
//! (`__luish_internal savestate`).
//!
//! The state is the working directory, the file mode mask, variables (with
//! their export and readonly attributes), traps, functions, aliases and
//! options. It is written as shell commands, so it is restored by running
//! them with `.`. Restoring sets everything that was saved, but doesn't
//! remove what wasn't (such as variables set since).

use crate::builtins::single_quote;
use crate::options::{OPTIONS, Opt};
use crate::shell::Shell;
use crate::{signals, sys, unparse};

/// Options that describe how the shell was started or runs, rather than
/// settings to restore.
const MODE_OPTIONS: &[Opt] = &[Opt::Interactive, Opt::Stdin, Opt::Monitor, Opt::Noexec];

/// Variables that belong to the process.
const PROCESS_VARS: &[&[u8]] = &[b"PPID", b"LINENO"];

impl Shell {
    /// The shell's state, as commands. The order matters when they are run:
    /// the directory comes first (so that the saved `PWD` and `OLDPWD` win),
    /// functions come before aliases (which would otherwise be expanded
    /// in their bodies; command names that are aliases are also quoted, for
    /// a shell that has them already), and options come last (so that `set -e`, `-u`,
    /// `-x` or `-a` don't affect the rest).
    pub fn dump_state(&self) -> Vec<u8> {
        let mut out = Vec::new();
        // `command`, in case a function of the same name is defined.
        if let Some(dir) = &self.curdir {
            out.extend_from_slice(b"command cd -- ");
            out.extend(single_quote(dir));
            out.push(b'\n');
        }
        let mask = sys::umask(0);
        sys::umask(mask);
        out.extend_from_slice(format!("command umask {mask:04o}\n").as_bytes());

        let vars = self.vars.sorted();
        for (name, var) in &vars {
            if PROCESS_VARS.contains(&name.as_slice()) {
                continue;
            }
            if var.exported {
                out.extend_from_slice(b"export ");
            } else if var.value.is_none() {
                continue;
            }
            out.extend_from_slice(name);
            if let Some(v) = &var.value {
                out.push(b'=');
                out.extend(single_quote(v));
            }
            out.push(b'\n');
        }
        for (name, var) in &vars {
            if var.readonly && !PROCESS_VARS.contains(&name.as_slice()) {
                out.extend_from_slice(b"readonly ");
                out.extend_from_slice(name);
                out.push(b'\n');
            }
        }

        for (sig, t) in self.traps.iter().enumerate() {
            if let Some(action) = t {
                out.extend_from_slice(b"trap -- ");
                out.extend(single_quote(action));
                out.push(b' ');
                out.extend_from_slice(signals::name(sig as i32).as_bytes());
                out.push(b'\n');
            }
        }

        let mut funcs: Vec<_> = self.functions.iter().collect();
        funcs.sort_by(|a, b| a.0.cmp(b.0));
        for (name, body) in funcs {
            // The name of a function can't be quoted to keep it from being
            // expanded as an alias (restored later).
            if self.aliases.contains_key(name) {
                out.extend_from_slice(b"command unalias ");
                out.extend(single_quote(name));
                out.extend_from_slice(b" 2>/dev/null\n");
            }
            out.extend(unparse::function(name, body, &self.aliases));
        }

        let mut aliases: Vec<_> = self.aliases.iter().collect();
        aliases.sort();
        for (name, value) in aliases {
            out.extend_from_slice(b"command alias ");
            out.extend(single_quote(name));
            out.push(b'=');
            out.extend(single_quote(value));
            out.push(b'\n');
        }

        for (o, _, name) in OPTIONS {
            if !MODE_OPTIONS.contains(o) {
                let sign = if self.options.get(*o) { '-' } else { '+' };
                out.extend_from_slice(format!("set {sign}o {name}\n").as_bytes());
            }
        }
        out
    }
}
