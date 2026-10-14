//! What the shell says about a command it didn't find.

use crate::shell::Shell;

impl Shell {
    /// Reports that the command `argv` wasn't found. All the places that
    /// find out call this, so that the message is the same whether the
    /// shell spawns, forks or execs.
    pub(super) fn report_not_found(&self, argv: &[Vec<u8>]) {
        self.error(format!("{}: not found", String::from_utf8_lossy(&argv[0])));
    }
}
