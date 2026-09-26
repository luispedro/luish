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
