//! `__luish_internal`: luish's own commands, as subcommands of one built-in
//! so that they don't take names from the command namespace. Widely used
//! ones may later get aliases of their own.

use crate::shell::{ExecResult, Shell};

type Subcommand = fn(&mut Shell, &[Vec<u8>]) -> ExecResult;

/// (name, function); each gets the arguments from its own name on.
const SUBCOMMANDS: &[(&[u8], Subcommand)] = &[
    (b"print-git-rev", print_git_rev),
    (b"print-git-rev-short", print_git_rev_short),
    (b"savestate", savestate),
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
