//! `__luish_internal`: luish's own commands, as subcommands of one built-in
//! so that they don't take names from the command namespace. Widely used
//! ones may later get aliases of their own.

use crate::shell::{ExecResult, Shell};

type Subcommand = fn(&mut Shell, &[Vec<u8>]) -> ExecResult;

/// (name, function); each gets the arguments from its own name on.
const SUBCOMMANDS: &[(&[u8], Subcommand)] = &[
    (b"bindkey", bindkey),
    (b"check-cache", crate::startcache::check),
    (b"clipcopy", super::clipcopy::clipcopy),
    (b"complete", complete),
    (b"function-file", function_file),
    (b"help", help),
    (b"plugin", plugin),
    (b"print", print),
    (b"print-git-rev", print_git_rev),
    (b"print-git-rev-short", print_git_rev_short),
    (b"savestate", savestate),
    (b"style", style),
];

/// The git revision luish was built from (see `build.rs`): the commit's
/// hash, with `-dirty` if the sources differed from it, or `unknown`.
pub const GIT_REV: &str = env!("LUISH_GIT_REV");
/// `GIT_REV` with the abbreviated hash.
pub const GIT_REV_SHORT: &str = env!("LUISH_GIT_REV_SHORT");
/// Identifies the sources luish was built from: `GIT_REV`, with a hash of
/// the sources if they were dirty.
pub const BUILD_ID: &str = env!("LUISH_BUILD_ID");

pub fn internal(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let Some(sub) = argv.get(1) else {
        sh.berr(&argv[0], "missing subcommand");
        return Ok(2);
    };
    match SUBCOMMANDS.iter().find(|s| s.0 == sub.as_slice()) {
        Some((_, f)) => f(sh, &argv[1..]),
        None => {
            sh.berr(
                &argv[0],
                format!("{}: unknown subcommand", String::from_utf8_lossy(sub)),
            );
            Ok(2)
        }
    }
}

/// `bindkey`: the `bindkey` built-in, which is also available here in
/// shells that aren't interactive.
fn bindkey(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    crate::interactive::keys::run(sh, b"__luish_internal bindkey", &argv[1..])
}

/// `style`: the `style` built-in, which is also available here in shells
/// that aren't interactive.
fn style(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    super::style::run(sh, b"__luish_internal style", &argv[1..])
}

/// `complete LINE`: prints the matches that Tab offers for the word at the
/// end of `LINE`, one per line: the text that replaces the word (with the
/// space or `/` that follows a single match), then a tab and the
/// description, if there is one. Status 1 if there are none, or if a
/// completer failed.
fn complete(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let Some(line) = argv.get(1) else {
        sh.berr(b"__luish_internal complete", "missing line");
        return Ok(2);
    };
    if let Some(a) = argv.get(2) {
        let msg = format!("too many arguments: {}", String::from_utf8_lossy(a));
        sh.berr(b"__luish_internal complete", msg);
        return Ok(2);
    }
    let Some(matches) = crate::interactive::completions(sh, line)? else {
        return Ok(1);
    };
    let mut out = Vec::new();
    for (text, desc) in &matches {
        out.extend_from_slice(text.as_bytes());
        if let Some(d) = desc {
            out.push(b'\t');
            out.extend_from_slice(d.as_bytes());
        }
        out.push(b'\n');
    }
    match sh.out_status(&out) {
        0 if matches.is_empty() => Ok(1),
        n => Ok(n),
    }
}

/// `help`: the `help` built-in, which is also available here in shells that
/// aren't interactive.
fn help(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    super::help::run(sh, b"__luish_internal help", &argv[1..])
}

/// `plugin`: the `plugin` built-in, which is also available here in shells
/// that aren't interactive.
fn plugin(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    crate::plugins::run(sh, b"__luish_internal plugin", &argv[1..])
}

/// `print`: the `print` built-in, which is also available here in shells
/// that aren't interactive.
fn print(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    super::print::run(sh, b"__luish_internal print", &argv[1..])
}

/// Fails with status 2 if there are arguments after the subcommand's name.
fn no_args(sh: &Shell, argv: &[Vec<u8>]) -> Result<(), i32> {
    match argv.get(1) {
        Some(a) => {
            let name = [b"__luish_internal ", argv[0].as_slice()].concat();
            sh.berr(&name, format!("too many arguments: {}", String::from_utf8_lossy(a)));
            Err(2)
        }
        None => Ok(()),
    }
}

/// `function-file NAME FILE [LINES [DIR]]`: records that the function was
/// defined in the file, for `BASH_SOURCE` while it runs and error messages
/// (written by `savestate`, as restoring a function defines it again).
/// `LINES` are the lines of its body in the file (`state::encode_lines`),
/// which its text written again doesn't have, and `DIR` the directory a
/// relative `FILE` is in. Status 1 if there is no such function.
fn function_file(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (name, file, lines, dir) = match argv {
        [_, name, file] => (name, file, None, None),
        [_, name, file, lines] => (name, file, Some(lines), None),
        [_, name, file, lines, dir] => (name, file, Some(lines), Some(dir)),
        _ => {
            sh.berr(
                b"__luish_internal function-file",
                "usage: function-file NAME FILE [LINES [DIR]]",
            );
            return Ok(2);
        }
    };
    if lines.is_some_and(|l| !l.iter().all(|&c| c.is_ascii_digit() || c == b',' || c == b'-')) {
        sh.berr(b"__luish_internal function-file", "bad line numbers");
        return Ok(2);
    }
    match sh.functions.get_mut(name) {
        Some(f) => {
            f.file = Some(crate::frames::SourceFile::new(file, dir.map(|d| &d[..])));
            // Without its lines, its text was written again, so its lines
            // aren't the file's. They are given to its body when it is
            // first called (`Shell::call_function`).
            f.lines_in_file = lines.is_some();
            f.pending_lines = lines.map(|l| l[..].into());
            Ok(0)
        }
        None => Ok(1),
    }
}

/// `print-git-rev`: prints the git revision luish was built from.
fn print_git_rev(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if let Err(e) = no_args(sh, argv) {
        return Ok(e);
    }
    Ok(sh.out_status(format!("{GIT_REV}\n").as_bytes()))
}

/// `print-git-rev-short`: the same, with the abbreviated hash.
fn print_git_rev_short(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if let Err(e) = no_args(sh, argv) {
        return Ok(e);
    }
    Ok(sh.out_status(format!("{GIT_REV_SHORT}\n").as_bytes()))
}

/// `savestate`: prints commands that restore the shell's state (see
/// `state.rs`).
fn savestate(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if let Err(e) = no_args(sh, argv) {
        return Ok(e);
    }
    Ok(sh.out_status(&sh.dump_state()))
}
