//! The last step of command lookup: what the shell says about a command it
//! didn't find, after `NAME: not found`. It runs only when the command
//! isn't found, so it costs nothing otherwise.

use crate::exec::simple::shell_quote;
use crate::options::Options;
use crate::shell::Shell;

/// A fallback for a command name: given the command's words, a hint for the
/// user, or `None` for no hint.
type Fallback = fn(&[Vec<u8>]) -> Option<String>;

/// Fallbacks by command name: commands from other shells that luish does
/// differently.
const FALLBACKS: &[(&[u8], Fallback)] = &[(b"shopt", shopt)];

impl Shell {
    /// Reports that the command `argv` wasn't found, with a hint if there is
    /// one for it.
    pub(super) fn report_not_found(&self, argv: &[Vec<u8>]) {
        let name = &argv[0];
        self.error(format!("{}: not found", String::from_utf8_lossy(name)));
        if let Some(hint) = FALLBACKS.iter().find(|f| f.0 == &name[..]).and_then(|f| (f.1)(argv)) {
            self.error(hint);
        }
    }
}

/// bash's `shopt`: `shopt -s NAME...` and `shopt -u NAME...` become
/// `setopt` and `unsetopt` when luish knows the names (as it does zsh's
/// `globstar` and `autocd`).
fn shopt(argv: &[Vec<u8>]) -> Option<String> {
    let generic = || Some("shopt is bash's; luish sets options with setopt and unsetopt".to_string());
    let mut on = None;
    let mut i = 1;
    while let Some(a) = argv.get(i).filter(|a| a.len() > 1 && a[0] == b'-') {
        i += 1;
        if a == b"--" {
            break;
        }
        for &c in &a[1..] {
            match c {
                b's' => on = Some(true),
                b'u' => on = Some(false),
                // `-o`: the names are `set -o`'s, which `setopt` also takes.
                b'o' => {}
                _ => return generic(),
            }
        }
    }
    let names = &argv[i..];
    let Some(on) = on else { return generic() };
    if names.is_empty() || names.iter().any(|n| Options::find(n).is_none()) {
        return generic();
    }
    let mut cmd = if on { "setopt" } else { "unsetopt" }.to_string();
    for n in names {
        cmd.push(' ');
        cmd.push_str(&String::from_utf8_lossy(&shell_quote(n)));
    }
    Some(format!("shopt is bash's; in luish, use: {cmd}"))
}

#[cfg(test)]
mod tests {
    use super::shopt;

    fn hint(args: &[&str]) -> String {
        let argv: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes().to_vec()).collect();
        shopt(&argv).unwrap()
    }

    #[test]
    fn shopt_hints() {
        assert_eq!(
            hint(&["shopt", "-s", "globstar", "autocd"]),
            "shopt is bash's; in luish, use: setopt globstar autocd"
        );
        assert_eq!(
            hint(&["shopt", "-u", "globstar"]),
            "shopt is bash's; in luish, use: unsetopt globstar"
        );
        assert_eq!(
            hint(&["shopt", "-so", "errexit"]),
            "shopt is bash's; in luish, use: setopt errexit"
        );
        let generic = "shopt is bash's; luish sets options with setopt and unsetopt";
        assert_eq!(hint(&["shopt"]), generic);
        assert_eq!(hint(&["shopt", "globstar"]), generic);
        assert_eq!(hint(&["shopt", "-s", "extglob"]), generic);
        assert_eq!(hint(&["shopt", "-q", "globstar"]), generic);
        assert_eq!(hint(&["shopt", "-s"]), generic);
    }
}
