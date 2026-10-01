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
    Pipefail,
    // luish's own options, in `EXTENDED`
    PromptPercent,
    TransientRprompt,
    Globstar,
    Bareglobqual,
    Autocd,
    HistIgnoreSpace,
    HistReduceBlanks,
    HistSaveNoDups,
    HistExpand,
    HistVerify,
    IncAppendHistory,
    ShareHistory,
    Autosuggest,
    AutoPushd,
    PushdIgnoreDups,
    PushdSilent,
}

/// Option table: (option, letter, long name), in dash's order (with POSIX's
/// `pipefail` and `hashall`, which Debian's dash lacks, where its `debug` is).
/// `$-` lists the letters in reverse order, as dash does.
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
    (Opt::Pipefail, None, "pipefail"),
    (Opt::Hashall, Some(b'h'), "hashall"),
];

/// luish's own options, beyond POSIX and dash: (option, name). They are set
/// only with `setopt` and `unsetopt`, as in zsh, so that `set -o` and `$-`
/// stay as in dash. All are off by default. Their names are grouped by
/// what they apply to, as `group.name`.
pub const EXTENDED: &[(Opt, &str)] = &[
    (Opt::PromptPercent, "prompt.percent"),
    (Opt::TransientRprompt, "prompt.transient_rprompt"),
    (Opt::Globstar, "glob.star"),
    (Opt::Bareglobqual, "glob.bare_qualifiers"),
    (Opt::Autocd, "cd.auto"),
    (Opt::HistIgnoreSpace, "history.ignore_space"),
    (Opt::HistReduceBlanks, "history.reduce_blanks"),
    (Opt::HistSaveNoDups, "history.save_no_dups"),
    (Opt::HistExpand, "history.expand"),
    (Opt::HistVerify, "history.verify"),
    (Opt::IncAppendHistory, "history.inc_append"),
    (Opt::ShareHistory, "history.share"),
    (Opt::Autosuggest, "editor.autosuggest"),
    (Opt::AutoPushd, "pushd.auto"),
    (Opt::PushdIgnoreDups, "pushd.ignore_dups"),
    (Opt::PushdSilent, "pushd.silent"),
];

/// Other names of luish's own options: zsh's, and those they had before
/// they were grouped.
const ALIASES: &[(Opt, &str)] = &[
    (Opt::PromptPercent, "promptpercent"),
    (Opt::TransientRprompt, "transientrprompt"),
    (Opt::Globstar, "globstar"),
    (Opt::Bareglobqual, "bareglobqual"),
    (Opt::Autocd, "autocd"),
    (Opt::HistIgnoreSpace, "histignorespace"),
    (Opt::HistReduceBlanks, "histreduceblanks"),
    (Opt::HistSaveNoDups, "histsavenodups"),
    (Opt::HistExpand, "banghist"),
    (Opt::HistExpand, "histexpand"),
    (Opt::HistVerify, "histverify"),
    (Opt::IncAppendHistory, "incappendhistory"),
    (Opt::ShareHistory, "sharehistory"),
    (Opt::Autosuggest, "autosuggest"),
    (Opt::AutoPushd, "autopushd"),
    (Opt::PushdIgnoreDups, "pushdignoredups"),
    (Opt::PushdSilent, "pushdsilent"),
];

/// The type of a setting that holds a value rather than being on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A non-negative decimal number.
    Number,
    /// Any string; a file name, for instance.
    Text,
}

/// Settings with a value, set with `setopt NAME=VALUE`: (name, the variable
/// that holds the value, its type). `unsetopt NAME` unsets the variable,
/// which gives the default back.
pub const VALUES: &[(&str, &[u8], Kind)] = &[
    ("history.file", b"HISTFILE", Kind::Text),
    ("history.size", b"HISTSIZE", Kind::Number),
    ("history.save_size", b"SAVEHIST", Kind::Number),
];

/// A setting found by name for `setopt` and `unsetopt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// An option, and whether the name means it is on.
    Flag(Opt, bool),
    /// A setting with a value: its variable and type.
    Value(&'static [u8], Kind),
}

/// Whether `s` has the form of a setting's name: names (as for variables)
/// separated by `.`.
pub fn is_setting_name(s: &[u8]) -> bool {
    s.split(|&c| c == b'.').all(crate::lexer::is_valid_name)
}

/// Parses the value of an option: `true`, `on`, `yes` or `1`, or `false`,
/// `off`, `no` or `0` (case doesn't matter).
pub fn parse_bool(s: &[u8]) -> Option<bool> {
    match s.to_ascii_lowercase().as_slice() {
        b"true" | b"on" | b"yes" | b"1" => Some(true),
        b"false" | b"off" | b"no" | b"0" => Some(false),
        _ => None,
    }
}

/// The group of a grouped name: `history` for `history.share`.
pub fn group_of(name: &str) -> Option<&str> {
    name.rsplit_once('.').map(|g| g.0)
}

/// The groups of luish's own settings, sorted, each once.
pub fn groups() -> Vec<&'static str> {
    let names = EXTENDED.iter().map(|o| o.1).chain(VALUES.iter().map(|v| v.0));
    let mut groups: Vec<&str> = names.filter_map(group_of).collect();
    groups.sort_unstable();
    groups.dedup();
    groups
}

/// Finds a group for `setopt -p`, by its name compared as the names of
/// settings are (case doesn't matter and `_` is ignored).
pub fn find_group(name: &[u8]) -> Option<&'static str> {
    let name = normalize(name);
    groups().into_iter().find(|g| normalize(g.as_bytes()) == name)
}

/// The name that inverts an option's: with `no` added to its last part
/// (`noglob`, `history.no_share`).
pub fn inverted(name: &[u8]) -> Vec<u8> {
    match name.iter().rposition(|&c| c == b'.') {
        Some(i) => [&name[..=i], b"no_", &name[i + 1..]].concat(),
        None => [b"no", name].concat(),
    }
}

/// A name without `_` and in lower case, as names are compared.
fn normalize(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|&&c| c != b'_')
        .map(u8::to_ascii_lowercase)
        .collect()
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    flags: u64,
}

impl Options {
    pub fn get(&self, o: Opt) -> bool {
        self.flags & (1 << o as u64) != 0
    }

    pub fn set(&mut self, o: Opt, on: bool) {
        if on {
            self.flags |= 1 << o as u64;
        } else {
            self.flags &= !(1 << o as u64);
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

    /// Finds a setting for `setopt` and `unsetopt`, named as in zsh: case
    /// doesn't matter, `_` is ignored, and for an option a `no` prefix
    /// (on the last part of a grouped name, as in `history.no_share`) is
    /// added or removed to invert it.
    pub fn find(name: &[u8]) -> Option<Setting> {
        let name = normalize(name);
        let flag = |n: &[u8]| {
            Self::all_names()
                .chain(ALIASES.iter().copied())
                .find(|o| normalize(o.1.as_bytes()) == n)
                .map(|o| o.0)
        };
        if let Some(o) = flag(&name) {
            return Some(Setting::Flag(o, true));
        }
        if let Some(v) = VALUES.iter().find(|v| normalize(v.0.as_bytes()) == name) {
            return Some(Setting::Value(v.1, v.2));
        }
        let leaf = name.iter().rposition(|&c| c == b'.').map_or(0, |i| i + 1);
        let (group, leaf) = name.split_at(leaf);
        let inverse = match leaf.strip_prefix(b"no") {
            Some(rest) => [group, rest].concat(),
            None => [group, b"no", leaf].concat(),
        };
        flag(&inverse).map(|o| Setting::Flag(o, false))
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
    use super::{EXTENDED, Kind, OPTIONS, Opt, Options, Setting, find_group, groups, inverted, is_setting_name};

    /// The completion plugin of luish-std-plugins lists the options that
    /// `luish -o` takes.
    #[test]
    fn std_completion_lists_the_options() {
        let shells = include_str!("../luish-std-plugins/completion/shells.rhai");
        let names = OPTIONS
            .iter()
            .map(|o| o.2)
            .filter(|&n| n != "interactive" && n != "stdin");
        for name in names.chain(EXTENDED.iter().map(|o| o.1)) {
            assert!(
                shells.contains(&format!("\"{name}\"")),
                "{name} is missing from shells.rhai"
            );
        }
    }

    #[test]
    fn zsh_names() {
        let flag = |o, on| Some(Setting::Flag(o, on));
        assert_eq!(Options::find(b"PROMPT_PERCENT"), flag(Opt::PromptPercent, true));
        assert_eq!(Options::find(b"no_Prompt_Percent"), flag(Opt::PromptPercent, false));
        assert_eq!(Options::find(b"errexit"), flag(Opt::Errexit, true));
        assert_eq!(Options::find(b"NO_GLOB"), flag(Opt::Noglob, true));
        assert_eq!(Options::find(b"glob"), flag(Opt::Noglob, false));
        assert_eq!(Options::find(b"clobber"), flag(Opt::Noclobber, false));
        assert_eq!(Options::find(b"bogus"), None);
        assert_eq!(Options::find(b""), None);
    }

    #[test]
    fn grouped_names() {
        let flag = |o, on| Some(Setting::Flag(o, on));
        assert_eq!(Options::find(b"history.share"), flag(Opt::ShareHistory, true));
        assert_eq!(Options::find(b"History.Save_No_Dups"), flag(Opt::HistSaveNoDups, true));
        assert_eq!(Options::find(b"history.no_share"), flag(Opt::ShareHistory, false));
        assert_eq!(Options::find(b"no_history.share"), None);
        assert_eq!(Options::find(b"share_history"), flag(Opt::ShareHistory, true));
        assert_eq!(Options::find(b"glob.star"), flag(Opt::Globstar, true));
        assert_eq!(
            Options::find(b"history.file"),
            Some(Setting::Value(b"HISTFILE", Kind::Text))
        );
        assert_eq!(Options::find(b"history.nofile"), None);
        assert_eq!(Options::find(b"history"), None);
    }

    #[test]
    fn setting_groups() {
        assert_eq!(groups(), ["cd", "editor", "glob", "history", "prompt", "pushd"]);
        assert_eq!(find_group(b"History"), Some("history"));
        assert_eq!(find_group(b"hist_ory"), Some("history"));
        assert_eq!(find_group(b"errexit"), None);
        assert_eq!(find_group(b"history.share"), None);
        assert_eq!(inverted(b"history.share"), b"history.no_share");
        assert_eq!(inverted(b"glob"), b"noglob");
    }

    #[test]
    fn setting_names() {
        assert!(is_setting_name(b"history.file"));
        assert!(is_setting_name(b"errexit"));
        assert!(!is_setting_name(b"history."));
        assert!(!is_setting_name(b".file"));
        assert!(!is_setting_name(b"a..b"));
        assert!(!is_setting_name(b"1a.b"));
    }
}
