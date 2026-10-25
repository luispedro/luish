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
    GlobNull,
    GlobDots,
    GlobFail,
    GlobFailAll,
    BraceExpand,
    SyntaxRepeat,
    SyntaxFdVars,
    Autocd,
    HistIgnoreSpace,
    HistReduceBlanks,
    HistSaveNoDups,
    HistExpand,
    HistVerify,
    IncAppendHistory,
    ShareHistory,
    Autosuggest,
    NoHighlight,
    HighlightPaths,
    AutoPushd,
    PushdIgnoreDups,
    PushdSilent,
    NoTermIntegration,
    VarsTrace,
    VarsTraceHistory,
    VarsTraceAfterStartup,
    StartupAutoCache,
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
    (Opt::GlobNull, "glob.null"),
    (Opt::GlobDots, "glob.dots"),
    (Opt::GlobFail, "glob.fail"),
    (Opt::GlobFailAll, "glob.fail_all"),
    (Opt::BraceExpand, "expand.braces"),
    (Opt::SyntaxRepeat, "syntax.repeat"),
    (Opt::SyntaxFdVars, "syntax.fd_vars"),
    (Opt::Autocd, "cd.auto"),
    (Opt::HistIgnoreSpace, "history.ignore_space"),
    (Opt::HistReduceBlanks, "history.reduce_blanks"),
    (Opt::HistSaveNoDups, "history.save_no_dups"),
    (Opt::HistExpand, "history.expand"),
    (Opt::HistVerify, "history.verify"),
    (Opt::IncAppendHistory, "history.inc_append"),
    (Opt::ShareHistory, "history.share"),
    (Opt::Autosuggest, "editor.autosuggest"),
    (Opt::NoHighlight, "editor.no_highlight"),
    (Opt::HighlightPaths, "highlight.paths"),
    (Opt::AutoPushd, "pushd.auto"),
    (Opt::PushdIgnoreDups, "pushd.ignore_dups"),
    (Opt::PushdSilent, "pushd.silent"),
    (Opt::NoTermIntegration, "terminal.no_integration"),
    (Opt::VarsTrace, "vars.trace"),
    (Opt::VarsTraceHistory, "vars.trace_history"),
    (Opt::VarsTraceAfterStartup, "vars.trace_after_startup"),
    (Opt::StartupAutoCache, "startup.auto_cache"),
];

/// Other names of luish's own options: zsh's, and those they had before
/// they were grouped.
const ALIASES: &[(Opt, &str)] = &[
    (Opt::PromptPercent, "promptpercent"),
    (Opt::TransientRprompt, "transientrprompt"),
    (Opt::Globstar, "globstar"),
    (Opt::Bareglobqual, "bareglobqual"),
    // zsh's `NULL_GLOB` and bash's `nullglob` (names are compared without
    // `_`).
    (Opt::GlobNull, "nullglob"),
    (Opt::GlobDots, "globdots"),
    // Also so that `nonomatch` is its inverse, not that of `match`.
    (Opt::GlobFail, "nomatch"),
    (Opt::GlobFailAll, "cshnullglob"),
    (Opt::BraceExpand, "braceexpand"),
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

/// What luish does for one of bash's `shopt` options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shopt {
    /// The setting of this name, which `shopt` sets as `setopt` does.
    Same(&'static str),
    /// Always on: luish behaves this way.
    On,
    /// Always off: luish has no such option.
    Off,
    /// `login_shell`: whether the shell is a login shell, which can't be
    /// changed.
    Login,
}

/// bash's `shopt` options, in the order bash lists them, and what luish does
/// for each.
pub const SHOPT: &[(&str, Shopt)] = &[
    ("autocd", Shopt::Same("cd.auto")),
    ("assoc_expand_once", Shopt::Off),
    ("cdable_vars", Shopt::Off),
    ("cdspell", Shopt::Off),
    ("checkhash", Shopt::Off),
    ("checkjobs", Shopt::Off),
    // The line editor follows the window's size.
    ("checkwinsize", Shopt::On),
    // A command of several lines is one history entry.
    ("cmdhist", Shopt::On),
    ("compat31", Shopt::Off),
    ("compat32", Shopt::Off),
    ("compat40", Shopt::Off),
    ("compat41", Shopt::Off),
    ("compat42", Shopt::Off),
    ("compat43", Shopt::Off),
    ("compat44", Shopt::Off),
    ("complete_fullquote", Shopt::Off),
    ("direxpand", Shopt::Off),
    ("dirspell", Shopt::Off),
    ("dotglob", Shopt::Same("glob.dots")),
    ("execfail", Shopt::Off),
    // Aliases are expanded in scripts too, as in dash.
    ("expand_aliases", Shopt::On),
    ("extdebug", Shopt::Off),
    ("extglob", Shopt::Off),
    ("extquote", Shopt::Off),
    ("failglob", Shopt::Same("glob.fail")),
    ("force_fignore", Shopt::Off),
    // Ranges compare bytes, whatever the locale.
    ("globasciiranges", Shopt::On),
    // `.*` matches `.` and `..`, as in dash.
    ("globskipdots", Shopt::Off),
    ("globstar", Shopt::Same("glob.star")),
    ("gnu_errfmt", Shopt::Off),
    // The history file is appended to.
    ("histappend", Shopt::On),
    ("histreedit", Shopt::Off),
    ("histverify", Shopt::Same("history.verify")),
    ("hostcomplete", Shopt::Off),
    ("huponexit", Shopt::Off),
    // Command substitutions keep `set -e`, as in dash.
    ("inherit_errexit", Shopt::On),
    ("interactive_comments", Shopt::On),
    ("lastpipe", Shopt::Off),
    // A command of several lines keeps its newlines in the history.
    ("lithist", Shopt::On),
    // Off, although `local` keeps the value (as in dash): `typeset` in a
    // function starts unset, as in zsh.
    ("localvar_inherit", Shopt::Off),
    ("localvar_unset", Shopt::Off),
    ("login_shell", Shopt::Login),
    ("mailwarn", Shopt::Off),
    ("no_empty_cmd_completion", Shopt::Off),
    ("nocaseglob", Shopt::Off),
    ("nocasematch", Shopt::Off),
    ("noexpand_translation", Shopt::Off),
    ("nullglob", Shopt::Same("glob.null")),
    ("patsub_replacement", Shopt::Off),
    // Completion plugins.
    ("progcomp", Shopt::On),
    ("progcomp_alias", Shopt::Off),
    ("promptvars", Shopt::On),
    ("restricted_shell", Shopt::Off),
    // `shift` reports a count beyond the positional parameters, as dash's
    // does.
    ("shift_verbose", Shopt::On),
    // `.` looks for a name without `/` in `PATH`.
    ("sourcepath", Shopt::On),
    ("varredir_close", Shopt::Off),
    // `echo` expands backslash escapes, as dash's does.
    ("xpg_echo", Shopt::On),
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
    use super::{
        EXTENDED, Kind, OPTIONS, Opt, Options, SHOPT, Setting, Shopt, find_group, groups, inverted, is_setting_name,
    };

    /// The completion plugin of luish-std-plugins lists bash's `shopt`
    /// options (for `bash -O`), which `shopt` takes.
    #[test]
    fn shopt_names() {
        let shells = include_str!("../luish-std-plugins/completion/shells.rhai");
        let list = shells.split("fn shopt_options() {").nth(1).unwrap();
        let list = &list[..list.find(']').unwrap()];
        let names: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
        assert_eq!(names, SHOPT.iter().map(|o| o.0).collect::<Vec<_>>());
        for &(_, o) in SHOPT {
            if let Shopt::Same(name) = o {
                assert!(matches!(Options::find(name.as_bytes()), Some(Setting::Flag(_, true))));
            }
        }
    }

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
        // zsh's names, as `setopt` takes them, and bash's `nullglob`.
        assert_eq!(Options::find(b"NULL_GLOB"), flag(Opt::GlobNull, true));
        assert_eq!(Options::find(b"nullglob"), flag(Opt::GlobNull, true));
        assert_eq!(Options::find(b"glob.no_null"), flag(Opt::GlobNull, false));
        assert_eq!(Options::find(b"nomatch"), flag(Opt::GlobFail, true));
        assert_eq!(Options::find(b"NO_NOMATCH"), flag(Opt::GlobFail, false));
        assert_eq!(Options::find(b"csh_null_glob"), flag(Opt::GlobFailAll, true));
        assert_eq!(
            Options::find(b"history.file"),
            Some(Setting::Value(b"HISTFILE", Kind::Text))
        );
        assert_eq!(Options::find(b"history.nofile"), None);
        assert_eq!(Options::find(b"history"), None);
    }

    #[test]
    fn setting_groups() {
        assert_eq!(
            groups(),
            [
                "cd",
                "editor",
                "expand",
                "glob",
                "highlight",
                "history",
                "prompt",
                "pushd",
                "startup",
                "syntax",
                "terminal",
                "vars"
            ]
        );
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
