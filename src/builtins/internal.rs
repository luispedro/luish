//! `__luish_internal`: luish's own commands, as subcommands of one built-in
//! so that they don't take names from the command namespace. Widely used
//! ones may later get aliases of their own.

use crate::shell::{ExecResult, Shell};

type Subcommand = fn(&mut Shell, &[Vec<u8>]) -> ExecResult;

/// (name, function); each gets the arguments from its own name on.
const SUBCOMMANDS: &[(&[u8], Subcommand)] = &[(b"savestate", savestate)];

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

/// `savestate`: prints commands that restore the shell's state (see
/// `state.rs`).
fn savestate(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if let Some(a) = argv.get(1) {
        sh.berr(
            b"__luish_internal savestate",
            format!("too many arguments: {}", String::from_utf8_lossy(a)),
        );
        return Ok(2);
    }
    Ok(sh.out_status(&sh.dump_state()))
}
