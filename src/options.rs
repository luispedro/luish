//! Shell options (`set -e`, `set -o noclobber`, ...).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opt {
    Errexit,
    Noglob,
    Hashall,
    Interactive,
    Monitor,
    Noexec,
    Stdin,
    Nounset,
    Verbose,
    Xtrace,
    Allexport,
    Notify,
    Noclobber,
    Ignoreeof,
    Vi,
    Emacs,
    Nolog,
    Privileged,
    // luish's own options, in `EXTENDED`
    PromptPercent,
    Globstar,
    Bareglobqual,
    Autocd,
    HistIgnoreSpace,
    HistReduceBlanks,
    HistSaveNoDups,
    IncAppendHistory,
    ShareHistory,
    Autosuggest,
}

/// Option table: (option, letter, long name), in dash's order. `$-` lists
/// the letters in reverse order, as dash does.
pub const OPTIONS: &[(Opt, Option<u8>, &str)] = &[
    (Opt::Errexit, Some(b'e'), "errexit"),
    (Opt::Noglob, Some(b'f'), "noglob"),
    (Opt::Ignoreeof, Some(b'I'), "ignoreeof"),
    (Opt::Interactive, Some(b'i'), "interactive"),
    (Opt::Monitor, Some(b'm'), "monitor"),
    (Opt::Noexec, Some(b'n'), "noexec"),
    (Opt::Stdin, Some(b's'), "stdin"),
    (Opt::Xtrace, Some(b'x'), "xtrace"),
    (Opt::Verbose, Some(b'v'), "verbose"),
    (Opt::Vi, Some(b'V'), "vi"),
    (Opt::Emacs, Some(b'E'), "emacs"),
    (Opt::Noclobber, Some(b'C'), "noclobber"),
    (Opt::Allexport, Some(b'a'), "allexport"),
    (Opt::Notify, Some(b'b'), "notify"),
    (Opt::Nounset, Some(b'u'), "nounset"),
    (Opt::Privileged, Some(b'p'), "privileged"),
    (Opt::Nolog, None, "nolog"),
    (Opt::Hashall, Some(b'h'), "hashall"),
];

/// luish's own options, beyond POSIX and dash: (option, name). They are set
/// only with `setopt` and `unsetopt`, as in zsh, so that `set -o` and `$-`
/// stay as in dash. All are off by default.
pub const EXTENDED: &[(Opt, &str)] = &[
    (Opt::PromptPercent, "promptpercent"),
    (Opt::Globstar, "globstar"),
    (Opt::Bareglobqual, "bareglobqual"),
    (Opt::Autocd, "autocd"),
    (Opt::HistIgnoreSpace, "histignorespace"),
    (Opt::HistReduceBlanks, "histreduceblanks"),
    (Opt::HistSaveNoDups, "histsavenodups"),
    (Opt::IncAppendHistory, "incappendhistory"),
    (Opt::ShareHistory, "sharehistory"),
    (Opt::Autosuggest, "autosuggest"),
];

#[derive(Debug, Default, Clone)]
pub struct Options {
    flags: u32,
}

impl Options {
    pub fn get(&self, o: Opt) -> bool {
        self.flags & (1 << o as u32) != 0
    }

    pub fn set(&mut self, o: Opt, on: bool) {
        if on {
            self.flags |= 1 << o as u32;
        } else {
            self.flags &= !(1 << o as u32);
        }
        // vi and emacs editing modes are mutually exclusive
        if on && o == Opt::Vi {
            self.set(Opt::Emacs, false);
        } else if on && o == Opt::Emacs {
            self.set(Opt::Vi, false);
        }
    }

    pub fn by_letter(c: u8) -> Option<Opt> {
        OPTIONS.iter().find(|o| o.1 == Some(c)).map(|o| o.0)
    }

    pub fn by_name(name: &[u8]) -> Option<Opt> {
        OPTIONS.iter().find(|o| o.2.as_bytes() == name).map(|o| o.0)
    }

    /// Finds an option for `setopt` and `unsetopt`: any option, named as in
    /// zsh (case doesn't matter, `_` is ignored, and a `no` prefix is added
    /// or removed to invert it). Returns the option and whether the name
    /// means it is on.
    pub fn by_zsh_name(name: &[u8]) -> Option<(Opt, bool)> {
        let name: Vec<u8> = name
            .iter()
            .filter(|&&c| c != b'_')
            .map(u8::to_ascii_lowercase)
            .collect();
        let find = |n: &[u8]| Self::all_names().find(|o| o.1.as_bytes() == n).map(|o| o.0);
        if let Some(o) = find(&name) {
            return Some((o, true));
        }
        match name.strip_prefix(b"no") {
            Some(rest) => find(rest).map(|o| (o, false)),
            None => find(&[b"no", name.as_slice()].concat()).map(|o| (o, false)),
        }
    }

    /// All options with their names, dash's then luish's own, for `setopt`
    /// and `unsetopt` without arguments.
    pub fn all_names() -> impl Iterator<Item = (Opt, &'static str)> {
        OPTIONS.iter().map(|o| (o.0, o.2)).chain(EXTENDED.iter().copied())
    }

    /// The value of `$-`.
    pub fn letters(&self) -> Vec<u8> {
        OPTIONS
            .iter()
            .filter(|o| self.get(o.0))
            .filter_map(|o| o.1)
            .rev()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Opt, Options};

    #[test]
    fn zsh_names() {
        assert_eq!(
            Options::by_zsh_name(b"PROMPT_PERCENT"),
            Some((Opt::PromptPercent, true))
        );
        assert_eq!(
            Options::by_zsh_name(b"no_Prompt_Percent"),
            Some((Opt::PromptPercent, false))
        );
        assert_eq!(Options::by_zsh_name(b"errexit"), Some((Opt::Errexit, true)));
        assert_eq!(Options::by_zsh_name(b"NO_GLOB"), Some((Opt::Noglob, true)));
        assert_eq!(Options::by_zsh_name(b"glob"), Some((Opt::Noglob, false)));
        assert_eq!(Options::by_zsh_name(b"clobber"), Some((Opt::Noclobber, false)));
        assert_eq!(Options::by_zsh_name(b"bogus"), None);
        assert_eq!(Options::by_zsh_name(b""), None);
    }
}
