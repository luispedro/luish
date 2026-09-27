//! Tab completion for the line editor.
//!
//! The completer (and the highlighter, in `highlight.rs`) never touches `Shell`: before each prompt the REPL gives it
//! a snapshot of the names it needs (`Names`). Completion goes in four steps:
//!
//! 1. `analyze` finds the word under the cursor, what kind of word it is (a
//!    command name, an argument, a redirection target, a variable name), the
//!    quoting in effect, and the words of its command.
//! 2. A generator lists `Candidate`s for it: command names, filenames,
//!    variable names, or what a command's arguments complete to: a plugin's
//!    completer (through `ShellHelper::ask`, the only call back into the
//!    shell) or `ARGS`.
//! 3. The candidates are matched against the text typed (`matches`).
//! 4. Each match becomes a replacement for the line: the text already typed
//!    is kept, and what is added is quoted for the quoting at the cursor.
//!
//! Tab then completes as much as the matches have in common, or else opens
//! the menu (`menu.rs`) to choose among them. rustyline is only ever given
//! one candidate, which it puts in the line.

use std::cell::RefCell;
use std::os::unix::ffi::OsStrExt;
use std::sync::{Arc, Mutex};

use rustyline::Changeset;
use rustyline::completion::{Completer, Pair};
use rustyline::hint::Hinter;
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use super::keys::Pending;
use super::menu::{self, Item, Menu};
use crate::path::{DirStamp, dir_stamps};
use crate::sys;

/// What the completer knows about the shell, refreshed before each prompt.
#[derive(Default)]
pub struct Names {
    pub functions: Vec<Vec<u8>>,
    /// The aliases' names and values.
    pub aliases: Vec<(Vec<u8>, Vec<u8>)>,
    pub vars: Vec<Vec<u8>>,
    pub path: Vec<u8>,
    pub home: Option<Vec<u8>>,
    /// The commands with a plugin's completer.
    pub completers: Vec<Vec<u8>>,
    /// The jobs' numbers and commands, from the current job on.
    pub jobs: Vec<(usize, Vec<u8>)>,
    /// The loaded plugins.
    pub plugins: Vec<Vec<u8>>,
    /// Where `plugin load` finds plugins by name.
    pub plugin_dir: Option<Vec<u8>>,
    pub cdpath: Vec<u8>,
    /// `setopt cd.auto`: directories are commands too.
    pub autocd: bool,
    /// The options `setopt` and `unsetopt` can change, named as they list
    /// them, and whether each is on.
    pub options: Vec<(&'static str, bool)>,
}

/// Runs the completer for a command (in `Names::completers`), given the
/// words of the command and the index of the one under the cursor (which
/// ends at the cursor).
pub type Ask = fn(&[Vec<u8>], usize) -> Completion;

#[derive(Default)]
pub struct ShellHelper {
    pub names: Names,
    pub highlight: super::highlight::State,
    pub ask: Option<Ask>,
    /// The prompt, as the line editor measures it (for the menu's height).
    pub prompt: String,
    pub menu: Arc<Mutex<Menu>>,
    /// Shared with the key bindings (see `keys.rs`).
    pub keys: Arc<Mutex<super::keys::State>>,
    /// Show autosuggestions (`setopt autosuggest`).
    pub suggest: bool,
    path_cache: RefCell<PathCache>,
}

/// What a command's completer gave.
pub enum Completion {
    /// Use the default completion (see `ARGS`).
    Default,
    /// The candidates, and where in the word the part they complete starts
    /// (the length of the prefix they leave alone, such as `--opt=`).
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    Candidates(usize, Vec<Candidate>),
    /// The completer failed, and printed why.
    Failed,
}

/// A possible completion of the word under the cursor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Candidate {
    /// The completed text, unquoted. It replaces the part of the word being
    /// completed, so it starts with the text typed if it is a match.
    pub value: Vec<u8>,
    /// What the menu shows, if not `value`: a file's name without its
    /// directory.
    pub display: Option<Vec<u8>>,
    /// Shown after the candidate in the menu.
    pub desc: Option<Vec<u8>>,
    /// What follows the candidate when it is the only match.
    pub suffix: Suffix,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Suffix {
    /// Nothing: the word goes on (a directory's `/` is part of its value).
    None,
    /// The closing quote, if the word is quoted, then this text (usually
    /// a space).
    Close(Vec<u8>),
}

impl Candidate {
    /// A candidate that ends the word.
    pub fn word(value: &[u8]) -> Candidate {
        Candidate {
            value: value.to_vec(),
            display: None,
            desc: None,
            suffix: Suffix::Close(b" ".to_vec()),
        }
    }
}

/// The executables found in `PATH`, rescanned when `PATH` or one of its
/// directories changes (see `path::dir_stamps`).
#[derive(Default)]
struct PathCache {
    path: Vec<u8>,
    stamps: Vec<DirStamp>,
    names: Vec<Vec<u8>>,
}

pub(super) const RESERVED: &[&[u8]] = &[
    b"case", b"do", b"done", b"elif", b"else", b"esac", b"fi", b"for", b"if", b"in", b"then", b"until", b"while",
];

/// Reserved words after which a command comes next.
const BEFORE_COMMAND: &[&[u8]] = &[b"if", b"then", b"else", b"elif", b"do", b"while", b"until", b"!", b"{"];

/// Commands that take another command as their argument (after options).
pub(super) const PRECOMMANDS: &[&[u8]] = &[
    b"command", b"exec", b"nohup", b"sudo", b"doas", b"env", b"time", b"nice", b"xargs",
];

/// Commands whose arguments can be assignments.
/// The name under which a plugin registers the completer for commands that
/// have none (as in zsh's `compdef -default-`).
pub const DEFAULT_COMPLETER: &[u8] = b"-default-";

const DECLARATIONS: &[&[u8]] = &[b"export", b"readonly", b"local"];

/// What the arguments of a command complete to, if not filenames.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Args {
    Dirs,
    /// Directories, or else those in `CDPATH`.
    Cd,
    /// Variable names (and filenames after `NAME=`).
    Vars,
    Commands,
    Builtins,
    /// Variable names, or function names after `-f`.
    Unset,
    Aliases,
    /// Job specs (`%1`).
    Jobs,
    /// Job specs, and signal names after `-` or `-s`.
    Kill,
    /// Signal names after the action.
    Trap,
    /// The subcommands of `plugin`, then plugins.
    Plugin,
    /// Variable names, except for the prompt after `-p`.
    Read,
    /// A variable name after the option string.
    Getopts,
    /// A variable name, then `in`.
    For,
    /// Widget names after the key sequence.
    Bindkey,
    /// Option names (those that are off for `setopt`, on for `unsetopt`),
    /// setting names, and filenames after `NAME=`.
    Setopt(bool),
}

const ARGS: &[(&[u8], Args)] = &[
    (b"cd", Args::Cd),
    (b"pushd", Args::Cd),
    (b"rmdir", Args::Dirs),
    (b"export", Args::Vars),
    (b"local", Args::Vars),
    (b"readonly", Args::Vars),
    (b"unset", Args::Unset),
    (b"hash", Args::Commands),
    (b"type", Args::Commands),
    (b"which", Args::Commands),
    (b"help", Args::Builtins),
    (b"alias", Args::Aliases),
    (b"unalias", Args::Aliases),
    (b"fg", Args::Jobs),
    (b"bg", Args::Jobs),
    (b"jobs", Args::Jobs),
    (b"wait", Args::Jobs),
    (b"kill", Args::Kill),
    (b"trap", Args::Trap),
    (b"plugin", Args::Plugin),
    (b"read", Args::Read),
    (b"getopts", Args::Getopts),
    (b"for", Args::For),
    (b"bindkey", Args::Bindkey),
    (b"setopt", Args::Setopt(true)),
    (b"unsetopt", Args::Setopt(false)),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Quote {
    None,
    Single,
    Double,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Command,
    /// An argument of a command (`Word::words` is not empty).
    Arg,
    /// A redirection target or the value of an assignment.
    File,
    /// A variable name after `$` (or after `${`, when the flag is set).
    Var(bool),
    Nothing,
}

/// The word under the cursor.
#[derive(Debug)]
struct Word {
    /// Where the replaced text starts.
    start: usize,
    kind: Kind,
    quote: Quote,
    /// The unquoted text of the word up to the cursor.
    text: Vec<u8>,
    /// Where in `text` a filename starts (after `=` or `:`).
    split: usize,
    /// For each prefix `text[..j]`, where its raw text ends in the line and
    /// the quoting there.
    offsets: Vec<(usize, Quote)>,
    /// The words of the command before this one, unquoted, starting with
    /// the command name. A word containing a substitution is empty.
    words: Vec<Vec<u8>>,
}

impl Word {
    /// Whether `text[j]` is a character on its own in the line, unquoted
    /// (and so a `~` there is a tilde prefix).
    fn unquoted(&self, j: usize) -> bool {
        match (self.offsets.get(j), self.offsets.get(j + 1)) {
            (Some(a), Some(b)) => b.0 == a.0 + 1 && b.1 == Quote::None,
            _ => false,
        }
    }
}

/// The tokenizer's state while scanning the line up to the cursor.
struct Scan<'a> {
    /// Whether the next word is a command name.
    cmd_pos: bool,
    /// Whether the next word is the target of a redirection.
    redirect: bool,
    /// Whether the last command word takes a command as its argument.
    precommand: bool,
    /// The state outside each open `$(`, `(` or backquote: its quoting,
    /// whether it was opened by a backquote, and its command's words.
    stack: Vec<(Quote, bool, Vec<Vec<u8>>)>,
    /// Start of the current word, if in one.
    start: Option<usize>,
    /// The current word, unquoted.
    text: Vec<u8>,
    /// Where in `text` the part to complete begins (after `=` or `:`).
    split: usize,
    /// As in `Word`.
    offsets: Vec<(usize, Quote)>,
    /// Whether the current word contains a substitution.
    subst: bool,
    quote: Quote,
    /// The words of the current command so far.
    words: Vec<Vec<u8>>,
    /// Whether the current word has quotes or backslashes (and so is not
    /// an alias).
    quoted: bool,
    /// The aliases (name and value), and those being expanded.
    aliases: &'a [(Vec<u8>, Vec<u8>)],
    expanding: Vec<Vec<u8>>,
    /// Whether the next word is an alias even if it is not a command name
    /// (after an alias whose value ends with a blank).
    alias_next: bool,
    /// Whether this scans the words after the cursor (`words_after`): it
    /// stops at the end of the command.
    after: bool,
    /// Whether the next word is left out of `words` (the rest of the word
    /// under the cursor).
    skip: bool,
    /// Whether a comment started.
    comment: bool,
    /// Whether to stop scanning.
    done: bool,
}

impl<'a> Scan<'a> {
    fn new(aliases: &'a [(Vec<u8>, Vec<u8>)]) -> Scan<'a> {
        Scan {
            cmd_pos: true,
            redirect: false,
            precommand: false,
            stack: Vec::new(),
            start: None,
            text: Vec::new(),
            split: 0,
            offsets: Vec::new(),
            subst: false,
            quote: Quote::None,
            words: Vec::new(),
            quoted: false,
            aliases,
            expanding: Vec::new(),
            alias_next: false,
            after: false,
            skip: false,
            comment: false,
            done: false,
        }
    }

    /// Starts a word at `at` in the line, unless in one.
    fn begin(&mut self, at: usize) {
        if self.start.is_none() {
            self.start = Some(at);
            self.split = 0;
            self.offsets = vec![(at, self.quote)];
        }
    }

    /// Adds `c` to the word, whose raw text now ends at `end`.
    fn push(&mut self, c: u8, end: usize) {
        self.text.push(c);
        self.offsets.push((end, self.quote));
    }

    /// Ends the current word, updating the command-position state.
    fn end_word(&mut self) {
        if self.start.take().is_none() {
            return;
        }
        let mut w = std::mem::take(&mut self.text);
        self.offsets.clear();
        if std::mem::take(&mut self.subst) {
            w.clear();
        }
        let quoted = std::mem::take(&mut self.quoted);
        let alias_next = std::mem::take(&mut self.alias_next);
        if std::mem::take(&mut self.skip) {
            return;
        }
        let alias = (alias_next || self.cmd_pos) && !quoted && !BEFORE_COMMAND.contains(&&w[..]);
        if self.redirect {
            self.redirect = false;
        } else if let Some(value) = self.alias(&w).filter(|_| alias) {
            self.expand_alias(w, value);
        } else if !self.cmd_pos {
            self.words.push(w);
        } else if is_assignment(&w) || (self.precommand && w.starts_with(b"-")) {
            // no change
        } else if BEFORE_COMMAND.contains(&&w[..]) {
            self.precommand = false;
            self.words.clear();
        } else {
            self.precommand = PRECOMMANDS.contains(&&w[..]);
            self.cmd_pos = self.precommand;
            self.words = vec![w];
        }
    }

    /// The value of the alias `name`, unless it is being expanded.
    fn alias(&self, name: &[u8]) -> Option<&'a [u8]> {
        let aliases: &'a [(Vec<u8>, Vec<u8>)] = self.aliases;
        let (_, value) = aliases.iter().find(|a| a.0 == name)?;
        (!self.expanding.iter().any(|e| e == name)).then_some(&value[..])
    }

    /// Scans the value of an alias in place of its name, a word that has
    /// just ended.
    fn expand_alias(&mut self, name: Vec<u8>, value: &'a [u8]) {
        let quote = self.quote;
        self.expanding.push(name);
        self.feed(value);
        self.end_word();
        self.expanding.pop();
        self.quote = quote;
        self.done = false;
        self.comment = false;
        self.alias_next = value.last().is_some_and(|&c| c == b' ' || c == b'\t');
    }

    /// Ends the current command, at an operator.
    fn end_command(&mut self) {
        self.end_word();
        if self.after && self.stack.is_empty() {
            self.done = true;
            return;
        }
        self.cmd_pos = true;
        self.precommand = false;
        self.redirect = false;
        self.words.clear();
    }

    /// Forgets the current word.
    fn drop_word(&mut self) {
        self.start = None;
        self.text.clear();
        self.offsets.clear();
        self.subst = false;
    }

    /// Starts a nested command (`$(`, `(` or a backquote). The word it is
    /// in goes on after it.
    fn open(&mut self, backquote: bool) {
        self.drop_word();
        self.stack
            .push((self.quote, backquote, std::mem::take(&mut self.words)));
        self.quote = Quote::None;
        self.cmd_pos = true;
        self.precommand = false;
        self.redirect = false;
    }

    /// Ends a nested command, whose raw text ends at `end`. A word
    /// containing a substitution counts as empty, and the rest of it is
    /// completed as if it were the whole word.
    fn close(&mut self, end: usize) {
        self.drop_word();
        if let Some((q, _, words)) = self.stack.pop() {
            self.quote = q;
            self.words = words;
        }
        self.cmd_pos = false;
        self.begin(end);
        self.subst = true;
    }

    fn in_backquote(&self) -> bool {
        self.stack.last().is_some_and(|s| s.1)
    }

    /// Whether the current word is an assignment, where filenames are
    /// completed after `=` and `:`.
    fn in_assignment(&self) -> bool {
        let Some(eq) = self.text.iter().position(|&c| c == b'=') else {
            return false;
        };
        let name = &self.text[..eq];
        match self.words.first() {
            _ if self.redirect => false,
            _ if self.cmd_pos => crate::lexer::is_valid_name(name),
            Some(c) if c == b"setopt" => crate::options::is_setting_name(name),
            Some(c) => DECLARATIONS.contains(&&c[..]) && crate::lexer::is_valid_name(name),
            None => false,
        }
    }

    /// Scans `line`, until the end or until `done` is set.
    fn feed(&mut self, line: &[u8]) {
        let mut i = 0;
        while i < line.len() && !self.done {
            let c = line[i];
            let next = line.get(i + 1).copied();
            i += 1;
            match (self.quote, c) {
                (Quote::Single, b'\'') | (Quote::Double, b'"') => self.quote = Quote::None,
                (Quote::Single, _) => self.push(c, i),
                (Quote::Double | Quote::None, b'$') if next == Some(b'(') => {
                    self.open(false);
                    i += 1;
                }
                (Quote::Double | Quote::None, b'`') => {
                    if self.in_backquote() {
                        self.close(i);
                    } else {
                        self.open(true);
                    }
                }
                (Quote::Double, b'\\') if next.is_some_and(|n| b"$`\"\\\n".contains(&n)) => {
                    i += 1;
                    self.push(line[i - 1], i);
                }
                (Quote::Double, _) => self.push(c, i),
                (Quote::None, b' ' | b'\t') => self.end_word(),
                (Quote::None, b'\n' | b';' | b'&' | b'|') => self.end_command(),
                (Quote::None, b'(') => self.open(false),
                (Quote::None, b')') if self.after && self.stack.is_empty() => {
                    self.end_word();
                    self.done = true;
                }
                (Quote::None, b')') => self.close(i),
                (Quote::None, b'<' | b'>') => {
                    // A word of digits before the operator is an fd number.
                    if !self.text.iter().all(u8::is_ascii_digit) {
                        self.end_word();
                    }
                    self.skip = false;
                    self.start = None;
                    self.text.clear();
                    self.offsets.clear();
                    while i < line.len() && b"<>&|-".contains(&line[i]) {
                        i += 1;
                    }
                    self.redirect = true;
                }
                (Quote::None, b'#') if self.start.is_none() => {
                    self.comment = true;
                    self.done = true;
                }
                (Quote::None, _) => {
                    self.begin(i - 1);
                    self.quoted |= b"\\'\"".contains(&c);
                    match c {
                        b'\\' => {
                            if let Some(n) = next {
                                i += 1;
                                self.push(n, i);
                            }
                        }
                        b'\'' => self.quote = Quote::Single,
                        b'"' => self.quote = Quote::Double,
                        _ => {
                            self.push(c, i);
                            let assignment = self.in_assignment();
                            if (c == b'=' && (assignment || self.text.starts_with(b"--"))) || (c == b':' && assignment)
                            {
                                self.split = self.text.len();
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Finds the word ending at the end of `line` and what it should complete to.
/// This is a rough tokenizer: it follows quoting, operators, redirections,
/// assignments, and substitutions, but not the full grammar.
fn analyze(line: &[u8], aliases: &[(Vec<u8>, Vec<u8>)]) -> Word {
    let mut s = Scan::new(aliases);
    s.feed(line);
    if s.comment {
        return Word {
            start: line.len(),
            kind: Kind::Nothing,
            quote: Quote::None,
            text: Vec::new(),
            split: 0,
            offsets: vec![(line.len(), Quote::None)],
            words: Vec::new(),
        };
    }

    // A variable name after `$` or `${` (not inside single quotes).
    if s.quote != Quote::Single {
        let n = line
            .iter()
            .rev()
            .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
            .count();
        let name_start = line.len() - n;
        let brace = line[..name_start].ends_with(b"${");
        if brace || line[..name_start].ends_with(b"$") {
            let dollar = name_start - if brace { 2 } else { 1 };
            let escaped = line[..dollar].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1;
            if !escaped && !line[name_start..].first().is_some_and(u8::is_ascii_digit) {
                return Word {
                    start: name_start,
                    kind: Kind::Var(brace),
                    quote: s.quote,
                    text: line[name_start..].to_vec(),
                    split: 0,
                    offsets: (name_start..=line.len()).map(|e| (e, Quote::None)).collect(),
                    words: Vec::new(),
                };
            }
        }
    }

    let kind = if s.redirect || (s.cmd_pos && is_assignment(&s.text)) {
        Kind::File
    } else if s.cmd_pos {
        Kind::Command
    } else {
        Kind::Arg
    };
    if s.start.is_none() {
        s.offsets = vec![(line.len(), s.quote)];
    }
    debug_assert_eq!(s.offsets.len(), s.text.len() + 1);
    Word {
        start: s.start.unwrap_or(line.len()),
        kind,
        quote: s.quote,
        text: s.text,
        split: s.split,
        offsets: s.offsets,
        words: s.words,
    }
}

/// The words of the command after the cursor, unquoted, given the text
/// after it (`rest`) and the quoting at the cursor. The rest of the word
/// under the cursor is left out.
fn words_after(rest: &[u8], quote: Quote) -> Vec<Vec<u8>> {
    let mut s = Scan::new(&[]);
    s.cmd_pos = false;
    s.after = true;
    s.quote = quote;
    if quote != Quote::None || rest.first().is_some_and(|c| !b" \t\n;&|<>()".contains(c)) {
        s.begin(0);
        s.skip = true;
    }
    s.feed(rest);
    s.end_word();
    s.words
}

/// Whether `w` starts with `NAME=`.
fn is_assignment(w: &[u8]) -> bool {
    match w.iter().position(|&c| c == b'=') {
        Some(eq) => crate::lexer::is_valid_name(&w[..eq]),
        None => false,
    }
}

/// How well `value` matches the text typed, if it does: 0 if it starts
/// with it, 1 if it does ignoring case, and 2 if the last part of it (after
/// a `/`) is found in the last part of `value`, ignoring case. A lowercase
/// letter typed matches either case, but an uppercase one only itself.
fn rank(value: &[u8], typed: &[u8]) -> Option<u8> {
    let same = |v: &u8, t: &u8| v == t || (t.is_ascii_lowercase() && *v == t.to_ascii_uppercase());
    let starts = |v: &[u8], t: &[u8]| v.len() >= t.len() && v.iter().zip(t).all(|(v, t)| same(v, t));
    if value.starts_with(typed) {
        return Some(0);
    }
    if starts(value, typed) {
        return Some(1);
    }
    let d = typed.iter().rposition(|&c| c == b'/').map_or(0, |i| i + 1);
    let (dir, last) = typed.split_at(d);
    if starts(value, dir) && (d..value.len()).any(|i| starts(&value[i..], last)) {
        return Some(2);
    }
    None
}

/// The candidates that match best (see `rank`).
fn best_matches<T>(items: Vec<T>, value: impl Fn(&T) -> &[u8], typed: &[u8]) -> Vec<T> {
    let ranked: Vec<(u8, T)> = items
        .into_iter()
        .filter_map(|c| Some((rank(value(&c), typed)?, c)))
        .collect();
    let Some(best) = ranked.iter().map(|r| r.0).min() else {
        return Vec::new();
    };
    ranked.into_iter().filter(|r| r.0 == best).map(|r| r.1).collect()
}

/// Quotes `s` for insertion where the quoting state is `quote`. `at_start`
/// says whether `s` begins the word, where `~` and `#` are special.
fn quote_suffix(s: &[u8], quote: Quote, at_start: bool, out: &mut Vec<u8>) {
    for (i, &c) in s.iter().enumerate() {
        match quote {
            Quote::None => match c {
                b'\n' => out.extend_from_slice(b"'\n'"),
                b' ' | b'\t' | b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')' | b'$' | b'`' | b'\\' | b'"' | b'\''
                | b'*' | b'?' | b'[' => {
                    out.push(b'\\');
                    out.push(c);
                }
                b'~' | b'#' if at_start && i == 0 => {
                    out.push(b'\\');
                    out.push(c);
                }
                _ => out.push(c),
            },
            Quote::Double => {
                if b"$`\"\\".contains(&c) {
                    out.push(b'\\');
                }
                out.push(c);
            }
            Quote::Single => {
                if c == b'\'' {
                    out.extend_from_slice(b"'\\''");
                } else {
                    out.push(c);
                }
            }
        }
    }
}

fn closing(quote: Quote) -> &'static [u8] {
    match quote {
        Quote::None => b"",
        Quote::Single => b"'",
        Quote::Double => b"\"",
    }
}

/// What is being completed: the word, the raw line, where in the word's
/// text the candidates' values start, and the quoting to insert with.
struct Target<'a> {
    w: &'a Word,
    line: &'a [u8],
    from: usize,
    quote: Quote,
}

impl Target<'_> {
    /// The text replacing the word in the line. The raw text for the part
    /// of the value already typed is kept (all of it, for a prefix match),
    /// then comes the quoted rest of the value and the suffix. If the raw
    /// text kept ends outside quotes, the rest is quoted as at the cursor.
    fn replacement(&self, c: &Candidate) -> Vec<u8> {
        let base = &self.w.text[self.from..];
        let k = c.value.iter().zip(base).take_while(|(a, b)| a == b).count();
        let (end, quote) = if k == base.len() {
            (self.line.len(), self.quote)
        } else {
            self.w.offsets[self.from + k]
        };
        let mut r = self.line[self.w.start..end].to_vec();
        let at_start = r.is_empty();
        let quote = if quote == Quote::None && self.quote != Quote::None {
            // (The closing quote is also the opening one.)
            r.extend_from_slice(closing(self.quote));
            self.quote
        } else {
            quote
        };
        quote_suffix(&c.value[k..], quote, at_start, &mut r);
        if let Suffix::Close(s) = &c.suffix {
            r.extend_from_slice(closing(quote));
            r.extend_from_slice(s);
        }
        r
    }
}

/// The candidates as the menu's items, sorted and without duplicates.
fn items(cands: &[Candidate], t: &Target) -> Vec<Item> {
    let mut out: Vec<Item> = (cands.iter())
        .filter_map(|c| {
            Some(Item {
                display: printable(c.display.as_ref().unwrap_or(&c.value)),
                desc: c.desc.as_deref().map(printable),
                replacement: String::from_utf8(t.replacement(c)).ok()?,
            })
        })
        .collect();
    out.sort_unstable_by(|a, b| a.replacement.cmp(&b.replacement));
    out.dedup_by(|a, b| a.replacement == b.replacement);
    out
}

/// `s` as text to show, with control characters as `?`.
fn printable(s: &[u8]) -> String {
    (String::from_utf8_lossy(s).chars())
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// The longest common prefix of the items' replacements.
fn common_prefix(items: &[Item]) -> &str {
    let first = &items[0].replacement;
    let mut n = (items[1..].iter())
        .map(|i| {
            first
                .bytes()
                .zip(i.replacement.bytes())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .min()
        .unwrap_or(first.len());
    while !first.is_char_boundary(n) {
        n -= 1;
    }
    &first[..n]
}

pub(super) fn is_executable(path: &[u8]) -> bool {
    sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG) && sys::access(path, libc::X_OK)
}

/// Whether `-f` (for functions) is in effect after the options in `args`
/// of `unset`: the last of `-f` and `-v` wins.
fn unset_functions(args: &[Vec<u8>]) -> bool {
    let mut functions = false;
    for a in args {
        match &a[..] {
            b"-f" => functions = true,
            b"-v" => functions = false,
            _ => break,
        }
    }
    functions
}

/// How many of `args` are operands, after the options (`getopts` has no
/// options, but takes `--`).
fn operands(args: &[Vec<u8>]) -> usize {
    args.len() - usize::from(args.first().is_some_and(|a| a == b"--"))
}

/// Signal names, after `prefix`.
fn signals(prefix: &[u8], out: &mut Vec<Candidate>) {
    out.extend(crate::signals::names().map(|n| Candidate::word(&[prefix, n.as_bytes()].concat())));
}

/// `~user/` for every user.
fn users(out: &mut Vec<Candidate>) {
    for u in sys::user_names() {
        let value = [&b"~"[..], &u, b"/"].concat();
        out.push(Candidate {
            display: Some(value[..value.len() - 1].to_vec()),
            desc: None,
            suffix: Suffix::None,
            value,
        });
    }
}

fn read_dir(dir: &[u8]) -> Vec<Vec<u8>> {
    let dir = if dir.is_empty() { b"." } else { dir };
    let Ok(rd) = std::fs::read_dir(std::ffi::OsStr::from_bytes(dir)) else {
        return Vec::new();
    };
    rd.flatten().map(|e| e.file_name().as_bytes().to_vec()).collect()
}

impl PathCache {
    fn refresh(&mut self, path: &[u8]) {
        let stamps = dir_stamps(path);
        if self.path == path && self.stamps == stamps {
            return;
        }
        let dirs: Vec<&[u8]> = path.split(|&c| c == b':').collect();
        self.names.clear();
        for d in &dirs {
            for name in read_dir(d) {
                let mut full = if d.is_empty() { b".".to_vec() } else { d.to_vec() };
                full.push(b'/');
                full.extend_from_slice(&name);
                if is_executable(&full) {
                    self.names.push(name);
                }
            }
        }
        self.names.sort_unstable();
        self.names.dedup();
        self.path = path.to_vec();
        self.stamps = stamps;
    }
}

/// Which files `ShellHelper::files` lists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Files {
    All,
    Executables,
    Dirs,
}

impl ShellHelper {
    /// Completes the word that ends at the end of `line`. `after` is the
    /// text after the cursor.
    fn complete_bytes(&self, line: &[u8], after: &[u8]) -> (usize, Vec<Item>) {
        let w = analyze(line, &self.names.aliases);
        let (from, cands) = match self.ask_completer(&w, after) {
            Completion::Default => self.generate(&w),
            Completion::Candidates(from, c) => (from.min(w.text.len()), c),
            Completion::Failed => {
                // The line is left as it is, but with one candidate
                // rustyline redraws it, below the error message.
                let typed = String::from_utf8_lossy(&line[w.start..]).into_owned();
                return (
                    w.start,
                    vec![Item {
                        display: typed.clone(),
                        desc: None,
                        replacement: typed,
                    }],
                );
            }
        };
        let cands = best_matches(cands, |c| &c.value, &w.text[from..]);
        // A variable name needs no quoting, and a `}` after it ends the
        // expansion, not the quoted word.
        let quote = if matches!(w.kind, Kind::Var(_)) {
            Quote::None
        } else {
            w.quote
        };
        let t = Target {
            w: &w,
            line,
            from,
            quote,
        };
        (w.start, items(&cands, &t))
    }

    /// What the completer of the word's command gives, if it has one, or
    /// else the default completer (`-default-`), for commands whose
    /// arguments luish doesn't know.
    fn ask_completer(&self, w: &Word, after: &[u8]) -> Completion {
        let has = |name: &[u8]| self.names.completers.iter().any(|c| c == name);
        let cmd = w.words.first().map_or(&b""[..], |c| c);
        match (w.kind, self.ask) {
            (Kind::Arg, Some(ask)) if has(cmd) || (has(DEFAULT_COMPLETER) && !ARGS.iter().any(|a| a.0 == cmd)) => {
                let mut words = w.words.clone();
                words.push(w.text.clone());
                words.extend(words_after(after, w.quote));
                ask(&words, w.words.len())
            }
            _ => Completion::Default,
        }
    }

    /// The candidates for the word, and where in its text the part they
    /// complete starts.
    fn generate(&self, w: &Word) -> (usize, Vec<Candidate>) {
        let mut out = Vec::new();
        let files = |which, out: &mut Vec<Candidate>| {
            let text = &w.text[w.split..];
            if text.starts_with(b"~") && !text.contains(&b'/') && w.unquoted(w.split) {
                users(out);
            } else {
                self.files(text, which, out);
            }
            w.split
        };
        let from = match w.kind {
            Kind::Nothing => 0,
            Kind::Var(brace) => {
                let suffix = if brace {
                    Suffix::Close(b"}".to_vec())
                } else {
                    Suffix::None
                };
                for v in &self.names.vars {
                    out.push(Candidate {
                        suffix: suffix.clone(),
                        ..Candidate::word(v)
                    });
                }
                0
            }
            Kind::Command if !w.text.contains(&b'/') => {
                self.commands(&mut out);
                if self.names.autocd {
                    self.cd_dirs(&w.text, &mut out);
                }
                0
            }
            Kind::Command => {
                self.files(&w.text, Files::Executables, &mut out);
                if self.names.autocd && out.iter().all(|c| c.suffix != Suffix::None) {
                    self.cdpath_dirs(&w.text, &mut out);
                }
                0
            }
            Kind::Arg => {
                let cmd = w.words.first().map_or(&b""[..], |c| &c[..]);
                let args = &w.words[1..];
                let words = |names: &[Vec<u8>], out: &mut Vec<Candidate>| {
                    out.extend(names.iter().map(|v| Candidate::word(v)));
                    0
                };
                match ARGS.iter().find(|a| a.0 == cmd).map(|a| a.1) {
                    Some(Args::Vars) if !w.text.contains(&b'=') => words(&self.names.vars, &mut out),
                    Some(Args::Unset) if unset_functions(args) => words(&self.names.functions, &mut out),
                    Some(Args::Unset) => words(&self.names.vars, &mut out),
                    Some(Args::Aliases) => {
                        out.extend(self.names.aliases.iter().map(|a| Candidate::word(&a.0)));
                        0
                    }
                    Some(Args::Commands) => {
                        self.commands(&mut out);
                        0
                    }
                    Some(Args::Builtins) => {
                        out.extend(crate::builtins::names().map(Candidate::word));
                        0
                    }
                    Some(Args::Jobs) => {
                        self.jobs(&w.text, &mut out);
                        0
                    }
                    Some(Args::Kill) => {
                        if w.text.starts_with(b"-") && args.is_empty() {
                            signals(b"-", &mut out);
                        } else if args.last().is_some_and(|a| a == b"-s") {
                            signals(b"", &mut out);
                        } else {
                            self.jobs(&w.text, &mut out);
                        }
                        0
                    }
                    Some(Args::Trap) if args.iter().filter(|a| *a != b"--").count() >= 1 => {
                        signals(b"", &mut out);
                        out.push(Candidate::word(b"EXIT"));
                        0
                    }
                    Some(Args::Read) if args.last().is_some_and(|a| a == b"-p") || w.text.starts_with(b"-") => 0,
                    Some(Args::Read) => words(&self.names.vars, &mut out),
                    Some(Args::Getopts) => match operands(args) {
                        0 => 0,
                        1 => words(&self.names.vars, &mut out),
                        _ => files(Files::All, &mut out),
                    },
                    Some(Args::For) => match args.len() {
                        0 => words(&self.names.vars, &mut out),
                        1 => words(&[b"in".to_vec()], &mut out),
                        _ => files(Files::All, &mut out),
                    },
                    Some(Args::Plugin) => match args.first().map(|a| &a[..]) {
                        None => words(
                            &[
                                b"list-available".to_vec(),
                                b"list-loaded".to_vec(),
                                b"load".to_vec(),
                                b"unload".to_vec(),
                            ],
                            &mut out,
                        ),
                        Some(b"load") if w.text.contains(&b'/') => files(Files::All, &mut out),
                        Some(b"load") => words(&self.plugin_files(), &mut out),
                        Some(b"unload") => words(&self.names.plugins, &mut out),
                        Some(_) => 0,
                    },
                    Some(Args::Bindkey) => {
                        let mut ops = 0;
                        let mut remove = false;
                        let mut it = args.iter();
                        while let Some(a) = it.next() {
                            match a.strip_prefix(b"-") {
                                Some(o) if ops == 0 && !o.is_empty() => {
                                    remove |= o.contains(&b'r');
                                    if o.ends_with(b"M") {
                                        it.next();
                                    }
                                }
                                _ => ops += 1,
                            }
                        }
                        if ops == 1 && !remove {
                            out.extend(super::keys::widget_names().map(|w| Candidate::word(w.as_bytes())));
                        }
                        0
                    }
                    Some(Args::Setopt(on)) if !w.text.contains(&b'=') => {
                        // As in zsh: the options the command would change,
                        // and after `no` (on the last part of a grouped
                        // name) the others inverted. Then the settings with
                        // a value.
                        let leaf = w.text.iter().rposition(|&c| c == b'.').map_or(0, |i| i + 1);
                        let no = w.text[leaf..].get(..2).is_some_and(|p| p.eq_ignore_ascii_case(b"no"));
                        for &(name, state) in &self.names.options {
                            if state != on {
                                out.push(Candidate::word(name.as_bytes()));
                            } else if no {
                                out.push(Candidate::word(&crate::options::inverted(name.as_bytes())));
                            }
                        }
                        let values = crate::options::VALUES.iter().map(|v| v.0.as_bytes());
                        out.extend(values.map(Candidate::word));
                        0
                    }
                    Some(Args::Dirs) => files(Files::Dirs, &mut out),
                    Some(Args::Cd) if w.text[w.split..].starts_with(b"~") => files(Files::Dirs, &mut out),
                    Some(Args::Cd) => {
                        self.cd_dirs(&w.text[w.split..], &mut out);
                        w.split
                    }
                    Some(Args::Vars | Args::Trap | Args::Setopt(_)) | None => files(Files::All, &mut out),
                }
            }
            Kind::File => files(Files::All, &mut out),
        };
        (from, out)
    }

    /// Command names: built-ins, reserved words, functions, aliases and
    /// the executables in `PATH`.
    fn commands(&self, out: &mut Vec<Candidate>) {
        let mut cache = self.path_cache.borrow_mut();
        cache.refresh(&self.names.path);
        // (The `map` shortens the built-in names' `'static` lifetime.)
        let all = crate::builtins::names()
            .map(|b: &[u8]| b)
            .chain(RESERVED.iter().copied())
            .chain(self.names.functions.iter().map(|c| &c[..]))
            .chain(self.names.aliases.iter().map(|a| &a.0[..]))
            .chain(cache.names.iter().map(|c| &c[..]));
        out.extend(all.map(Candidate::word));
    }

    /// Job specs, described by their commands: `%1`, or, if a name is
    /// typed after the `%`, the jobs' command names (those that give
    /// only one job).
    fn jobs(&self, typed: &[u8], out: &mut Vec<Candidate>) {
        let by_name = typed.len() > 1 && typed[0] == b'%' && !b"0123456789%+-?".contains(&typed[1]);
        for (n, text) in &self.names.jobs {
            let spec = if by_name {
                let name = text.split(|&c| c == b' ').next().unwrap_or_default();
                if self.names.jobs.iter().filter(|j| j.1.starts_with(name)).count() > 1 {
                    continue;
                }
                [b"%", name].concat()
            } else {
                format!("%{n}").into_bytes()
            };
            out.push(Candidate {
                desc: Some(text.clone()),
                ..Candidate::word(&spec)
            });
        }
    }

    /// The names of the plugins in the plugin directory: `.rhai` files and
    /// directories.
    fn plugin_files(&self) -> Vec<Vec<u8>> {
        (self.names.plugin_dir.as_deref()).map_or_else(Vec::new, crate::plugins::available_names)
    }

    /// The directories that `text` (a path, unquoted) could complete to as
    /// `cd`'s argument: those in the current directory, or else, as in zsh,
    /// those in `CDPATH`.
    fn cd_dirs(&self, text: &[u8], out: &mut Vec<Candidate>) {
        let start = out.len();
        self.files(text, Files::Dirs, out);
        if out.len() == start {
            self.cdpath_dirs(text, out);
        }
    }

    /// The directories in `CDPATH` that `text` could complete to, described
    /// by the directory they are in. As in `cd`, a name starting with `/`,
    /// `.` or `..` isn't looked for there.
    fn cdpath_dirs(&self, text: &[u8], out: &mut Vec<Candidate>) {
        let dotted = text == b"." || text == b".." || text.starts_with(b"./") || text.starts_with(b"../");
        if dotted || text.starts_with(b"/") || text.starts_with(b"~") || self.names.cdpath.is_empty() {
            return;
        }
        let start = out.len();
        for p in self.names.cdpath.split(|&c| c == b':') {
            // (The current directory's are already listed.)
            if p.is_empty() || p == b"." {
                continue;
            }
            let from = out.len();
            self.files_in(p, text, Files::Dirs, out);
            for c in &mut out[from..] {
                c.desc = Some(p.to_vec());
            }
        }
        // The first of the same name is the one `cd` goes to.
        let mut seen = std::collections::HashSet::new();
        let mut i = start;
        while i < out.len() {
            if seen.insert(out[i].value.clone()) {
                i += 1;
            } else {
                out.remove(i);
            }
        }
    }

    /// The files that `text` (a path, unquoted) could complete to. Dot
    /// files are listed only for a name starting with `.`.
    fn files(&self, text: &[u8], which: Files, out: &mut Vec<Candidate>) {
        self.files_in(b"", text, which, out);
    }

    /// As [`Self::files`], for a relative `text` from `base` (the
    /// current directory if empty).
    fn files_in(&self, base: &[u8], text: &[u8], which: Files, out: &mut Vec<Candidate>) {
        let slash = text.iter().rposition(|&c| c == b'/').map_or(0, |i| i + 1);
        let (typed_dir, prefix) = text.split_at(slash);
        let mut dir = typed_dir.to_vec();
        if let Some(rest) = dir.strip_prefix(b"~") {
            let user_end = rest.iter().position(|&c| c == b'/').unwrap_or(rest.len());
            let home = match &rest[..user_end] {
                b"" => self.names.home.clone(),
                user => sys::home_dir(user),
            };
            if let Some(home) = home {
                dir.splice(..1 + user_end, home);
            }
        }
        if !base.is_empty() {
            dir = [base, b"/", &dir].concat();
        }
        let mut names = read_dir(&dir);
        if prefix == b".." {
            names.push(b"..".to_vec());
        }
        names.retain(|n| !n.starts_with(b".") || prefix.starts_with(b"."));
        // Only the best matches are kept in the end, so the others need no
        // `stat`.
        for name in best_matches(names, |n| n, prefix) {
            let mut full = if dir.is_empty() { b"./".to_vec() } else { dir.clone() };
            full.extend_from_slice(&name);
            let is_dir = sys::stat(&full).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR);
            if is_dir {
                let display = [&name[..], b"/"].concat();
                out.push(Candidate {
                    value: [typed_dir, &display].concat(),
                    display: Some(display),
                    desc: None,
                    suffix: Suffix::None,
                });
            } else if which == Files::All || (which == Files::Executables && is_executable(&full)) {
                out.push(Candidate {
                    display: Some(name.clone()),
                    ..Candidate::word(&[typed_dir, &name].concat())
                });
            }
        }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    /// Completes the word at the cursor, or moves in the menu if it is open.
    /// rustyline puts the one candidate returned in the line and redraws it
    /// (with the menu, through `hint`). Also does `insert-last-word`, for the
    /// key bindings, since it needs the history.
    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        let pair = |replacement: &str| Pair {
            display: String::new(),
            replacement: replacement.to_owned(),
        };
        if let Ok(mut keys) = self.keys.lock()
            && let Some(p) = keys.pending.take()
        {
            return Ok(match p {
                Pending::InsertLastWord => {
                    let word = keys.insert_last_word(line, pos, ctx.history(), ctx.history_index());
                    word.map_or((pos, Vec::new()), |(start, w)| (start, vec![pair(&w)]))
                }
                // Made by `update`.
                Pending::Edit(e) => {
                    keys.edit = Some(e);
                    (pos, vec![pair("")])
                }
            });
        }
        let Ok(mut menu) = self.menu.lock() else {
            return Ok((pos, Vec::new()));
        };
        if let Some((start, text)) = menu.step(line, pos) {
            return Ok((start, vec![pair(&text)]));
        }
        // (Not held while a plugin's completer runs.)
        drop(menu);
        let (before, after) = line.as_bytes().split_at(pos);
        let (start, items) = self.complete_bytes(before, after);
        if items.len() < 2 {
            return Ok((start, items.iter().map(|i| pair(&i.replacement)).collect()));
        }
        let typed = line.get(start..pos).unwrap_or_default();
        let prefix = common_prefix(&items);
        if prefix.len() > typed.len() {
            return Ok((start, vec![pair(prefix)]));
        }
        let typed = pair(typed);
        if let Ok(mut menu) = self.menu.lock() {
            menu.open(line, start, pos, items);
        }
        Ok((start, vec![typed]))
    }

    /// Puts the candidate in the line, or makes the edit a key asked for.
    fn update(&self, line: &mut LineBuffer, start: usize, elected: &str, cl: &mut Changeset) {
        if let Some(e) = self.keys.lock().ok().and_then(|mut k| k.edit.take()) {
            line.replace(e.range, "", cl);
            line.set_pos(e.pos);
            return;
        }
        let end = line.pos();
        line.replace(start..end, elected, cl);
    }
}

impl Hinter for ShellHelper {
    type Hint = menu::Drawn;

    /// Draws the menu, if it is open, or else the autosuggestion. Also notes
    /// the position in the history for the key bindings.
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<menu::Drawn> {
        if let Ok(mut keys) = self.keys.lock() {
            keys.history_index = ctx.history_index();
            keys.history_len = ctx.history().len();
        }
        let colors = self.highlight.colors.as_ref();
        let sgr = |class, default: &str| {
            colors.map_or(default.to_owned(), |c| {
                String::from_utf8_lossy(c.sgr(class)).into_owned()
            })
        };
        let mut m = self.menu.lock().ok()?;
        if !m.is_open(line, pos) {
            drop(m);
            let rest = self.suggestion(line, pos, ctx.history())?;
            let style = sgr(super::highlight::Class::Suggest, "90");
            return Some(menu::Drawn {
                display: format!("\x1b[{style}m{rest}\x1b[0m"),
                completion: Some(rest),
            });
        }
        let (cols, rows) = sys::window_size(1).unwrap_or_default();
        let cols = if cols == 0 { 80 } else { cols };
        let rows = if rows == 0 { 24 } else { rows };
        let used = menu::rows(&[&self.prompt[..], line].concat(), cols);
        let style = menu::Style {
            select: sgr(super::highlight::Class::Select, "7"),
            desc: sgr(super::highlight::Class::Desc, ""),
        };
        Some(menu::Drawn {
            display: m.draw(cols, rows.saturating_sub(used), &style),
            completion: None,
        })
    }
}

impl ShellHelper {
    /// The autosuggestion, as zsh-autosuggestions makes it (with
    /// `setopt autosuggest`): the rest of the newest history entry that
    /// starts with the line, when the cursor is at its end.
    fn suggestion(&self, line: &str, pos: usize, history: &dyn rustyline::history::History) -> Option<String> {
        use rustyline::history::SearchDirection;
        if !self.suggest || pos != line.len() || line.trim().is_empty() {
            return None;
        }
        (0..history.len()).rev().find_map(|i| {
            let e = history.get(i, SearchDirection::Reverse).ok()??.entry;
            (e.len() > line.len() && e.starts_with(line)).then(|| e[line.len()..].to_owned())
        })
    }
}

impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(line: &str) -> (Kind, String) {
        let w = analyze(line.as_bytes(), &[]);
        (w.kind, String::from_utf8(w.text[w.split..].to_vec()).unwrap())
    }

    fn words(line: &str) -> Vec<String> {
        let w = analyze(line.as_bytes(), &[]);
        w.words.into_iter().map(|w| String::from_utf8(w).unwrap()).collect()
    }

    #[test]
    fn command_position() {
        use Kind::*;
        assert_eq!(kind("ec"), (Command, "ec".into()));
        assert_eq!(kind("echo hi; ca"), (Command, "ca".into()));
        assert_eq!(kind("echo ca"), (Arg, "ca".into()));
        assert_eq!(kind("x=1 y=2 ca"), (Command, "ca".into()));
        assert_eq!(kind("if tr"), (Command, "tr".into()));
        assert_eq!(kind("if true; then ec"), (Command, "ec".into()));
        assert_eq!(kind("a | b && c"), (Command, "c".into()));
        assert_eq!(kind("echo $(ca"), (Command, "ca".into()));
        assert_eq!(kind("echo \"`ca"), (Command, "ca".into()));
        assert_eq!(kind("sudo -E vi"), (Command, "vi".into()));
        assert_eq!(kind("(cd"), (Command, "cd".into()));
        assert_eq!(kind("cat <fo"), (File, "fo".into()));
        assert_eq!(kind("2>fo"), (File, "fo".into()));
        assert_eq!(kind("x >&2 fo"), (Arg, "fo".into()));
        assert_eq!(kind(">out ca"), (Command, "ca".into()));
        assert_eq!(kind("./a"), (Command, "./a".into()));
        assert_eq!(kind("echo # co"), (Nothing, "".into()));
    }

    #[test]
    fn quoting_and_splitting() {
        use Kind::*;
        assert_eq!(kind("ls 'a b"), (Arg, "a b".into()));
        assert_eq!(kind("ls a\\ b"), (Arg, "a b".into()));
        assert_eq!(kind("ls \"a\\$b"), (Arg, "a$b".into()));
        assert_eq!(kind("PATH=/bin:/usr/b"), (File, "/usr/b".into()));
        assert_eq!(kind("ls --file=fo"), (Arg, "fo".into()));
        assert_eq!(kind("ls a=b"), (Arg, "a=b".into()));
        assert_eq!(kind("export a=b:c"), (Arg, "c".into()));
        assert_eq!(kind("setopt history.file=~/h"), (Arg, "~/h".into()));
        assert_eq!(kind("export history.file=~/h"), (Arg, "history.file=~/h".into()));
        assert_eq!(kind("echo $HO"), (Var(false), "HO".into()));
        assert_eq!(kind("echo \"${HO"), (Var(true), "HO".into()));
        assert_eq!(kind("echo '$HO"), (Arg, "$HO".into()));
        assert_eq!(kind("echo \\$HO"), (Arg, "$HO".into()));
        let w = analyze(b"ls 'a b", &[]);
        assert_eq!((w.start, w.quote), (3, Quote::Single));
    }

    #[test]
    fn command_words() {
        assert_eq!(words("git -C 'my dir' ad"), ["git", "-C", "my dir"]);
        assert_eq!(words("sudo -E git ad"), ["git"]);
        assert_eq!(words("x=1 git >out ad"), ["git"]);
        assert_eq!(words("echo $(git ad"), ["git"]);
        assert_eq!(words("a; git ad"), ["git"]);
        assert_eq!(words("if git ad"), ["git"]);
        assert_eq!(words("git $(x) ad"), ["git", ""]);
        assert_eq!(words("$(x) ad"), [""]);
        assert_eq!(words("echo \"$(x)y\" ad"), ["echo", ""]);
        assert_eq!(words("echo a`x`b$(y) ad"), ["echo", ""]);
        assert_eq!(kind("echo \"$(x)y"), (Kind::Arg, "y".into()));
    }

    #[test]
    fn aliases() {
        let aliases = [
            (b"g".to_vec(), b"git".to_vec()),
            (b"gc".to_vec(), b"git -C 'my dir' commit".to_vec()),
            (b"ls".to_vec(), b"ls -F".to_vec()),
            (b"s".to_vec(), b"sudo ".to_vec()),
            (b"loop".to_vec(), b"loop2".to_vec()),
            (b"loop2".to_vec(), b"loop x".to_vec()),
            (b"cdg".to_vec(), b"cd /tmp; git".to_vec()),
            (b"w".to_vec(), b"watch ".to_vec()),
            (b"c".to_vec(), b"echo # g".to_vec()),
        ];
        let analyze = |line: &str| {
            let w = analyze(line.as_bytes(), &aliases);
            let words: Vec<_> = w.words.into_iter().map(|w| String::from_utf8(w).unwrap()).collect();
            (w.kind, words)
        };
        use Kind::*;
        assert_eq!(analyze("g ad"), (Arg, vec!["git".into()]));
        assert_eq!(analyze("g"), (Command, vec![]));
        assert_eq!(
            analyze("gc -m"),
            (Arg, vec!["git".into(), "-C".into(), "my dir".into(), "commit".into()])
        );
        assert_eq!(analyze("ls x"), (Arg, vec!["ls".into(), "-F".into()]));
        assert_eq!(analyze("s g ad"), (Arg, vec!["git".into()]));
        assert_eq!(analyze("s gi"), (Command, vec!["sudo".into()]));
        assert_eq!(analyze("loop a"), (Arg, vec!["loop".into(), "x".into()]));
        assert_eq!(analyze("cdg ad"), (Arg, vec!["git".into()]));
        assert_eq!(analyze("w g ad"), (Arg, vec!["watch".into(), "git".into()]));
        assert_eq!(analyze("echo g ad"), (Arg, vec!["echo".into(), "g".into()]));
        assert_eq!(analyze("\\g ad"), (Arg, vec!["g".into()]));
        assert_eq!(analyze("'g' ad"), (Arg, vec!["g".into()]));
        assert_eq!(analyze("c a"), (Arg, vec!["echo".into()]));
        assert_eq!(analyze("if g ad"), (Arg, vec!["git".into()]));
    }

    #[test]
    fn after_the_cursor() {
        let after = |rest: &str, quote| -> Vec<String> {
            (words_after(rest.as_bytes(), quote).into_iter())
                .map(|w| String::from_utf8(w).unwrap())
                .collect()
        };
        assert_eq!(after("", Quote::None), Vec::<String>::new());
        assert_eq!(after(" a 'b c'", Quote::None), ["a", "b c"]);
        assert_eq!(after("dd a", Quote::None), ["a"]);
        assert_eq!(after("d e' f", Quote::Single), ["f"]);
        assert_eq!(after(" a; b", Quote::None), ["a"]);
        assert_eq!(after(" a >out b | c", Quote::None), ["a", "b"]);
        assert_eq!(after("x>out b", Quote::None), ["b"]);
        assert_eq!(after("2>out b", Quote::None), ["b"]);
        assert_eq!(after(" a $(b; c) d) e", Quote::None), ["a", "", "d"]);
        assert_eq!(after("$(x)y z", Quote::None), ["z"]);
    }

    /// A completer for `git`, as a plugin could provide, and a default
    /// completer that knows `frob`.
    fn fake_git(words: &[Vec<u8>], i: usize) -> Completion {
        if words[0] != b"git" {
            return match &words[0][..] {
                b"frob" => Completion::Candidates(0, vec![Candidate::word(b"xylophone")]),
                _ => Completion::Default,
            };
        }
        match (i, &words[i][..]) {
            (_, w) if w.starts_with(b"--pretty=") => Completion::Candidates(
                b"--pretty=".len(),
                vec![Candidate::word(b"oneline"), Candidate::word(b"short")],
            ),
            (1, _) if words.len() > 2 => {
                assert_eq!(words[2..], [b"x".to_vec(), b"after".to_vec()]);
                Completion::Candidates(0, vec![])
            }
            (1, _) => Completion::Candidates(
                0,
                vec![
                    Candidate::word(b"add"),
                    Candidate::word(b"commit"),
                    Candidate {
                        suffix: Suffix::None,
                        ..Candidate::word(b"--color=")
                    },
                ],
            ),
            (_, b"boom") => Completion::Failed,
            _ => Completion::Default,
        }
    }

    fn complete(h: &ShellHelper, line: &str) -> Vec<String> {
        h.complete_bytes(line.as_bytes(), b"")
            .1
            .into_iter()
            .map(|p| p.replacement)
            .collect()
    }

    #[test]
    fn candidates() {
        let dir = std::env::temp_dir().join(format!("luish-complete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub dir")).unwrap();
        std::fs::write(dir.join("file one"), "").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        let exe = dir.join("sub dir/mytool");
        std::fs::write(&exe, "").unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let d = dir.to_str().unwrap();
        let h = ShellHelper {
            names: Names {
                functions: vec![b"myfunc".to_vec()],
                aliases: vec![(b"ll".to_vec(), b"ls -l".to_vec())],
                vars: vec![b"HOME".to_vec(), b"HOSTNAME".to_vec()],
                path: format!("{d}/sub dir").into_bytes(),
                home: Some(dir.as_os_str().as_bytes().to_vec()),
                completers: vec![b"git".to_vec()],
                jobs: vec![(2, b"vi notes".to_vec()), (1, b"sleep 10 | cat".to_vec())],
                plugins: vec![b"greet".to_vec()],
                plugin_dir: Some(dir.join("plugins").as_os_str().as_bytes().to_vec()),
                cdpath: format!("/nonexistent::{d}").into_bytes(),
                autocd: false,
                options: vec![
                    ("errexit", false),
                    ("noglob", false),
                    ("glob.star", true),
                    ("cd.auto", false),
                    ("history.share", true),
                    ("history.save_no_dups", false),
                ],
            },
            ask: Some(fake_git),
            ..Default::default()
        };
        assert_eq!(complete(&h, "myt"), ["mytool "]);
        assert_eq!(complete(&h, "myf"), ["myfunc "]);
        assert_eq!(complete(&h, "ech"), ["echo "]);
        assert_eq!(complete(&h, "whil"), ["while "]);
        assert_eq!(
            complete(&h, &format!("ls {d}/f")),
            [format!("ls {d}/file\\ one ")[3..].to_string()]
        );
        assert_eq!(complete(&h, &format!("ls '{d}/f")), [format!("'{d}/file one' ")]);
        assert_eq!(complete(&h, &format!("ls \"{d}/s")), [format!("\"{d}/sub dir/")]);
        assert_eq!(complete(&h, "ls ~/s"), ["~/sub\\ dir/"]);
        assert_eq!(complete(&h, "ls ~/"), ["~/file\\ one ", "~/sub\\ dir/"]);
        assert_eq!(complete(&h, "ls ~/."), ["~/.hidden "]);
        assert_eq!(complete(&h, "~/sub\\ dir/m"), ["~/sub\\ dir/mytool "]);
        assert_eq!(complete(&h, "~/f"), Vec::<String>::new());
        assert_eq!(complete(&h, "echo $HO"), ["HOME", "HOSTNAME"]);
        assert_eq!(complete(&h, "echo ${HOM"), ["HOME}"]);
        assert_eq!(complete(&h, "echo \"${HOM"), ["HOME}"]);
        assert_eq!(complete(&h, "X=~/f"), ["X=~/file\\ one "]);
        assert_eq!(
            complete(&h, "setopt history.s"),
            ["history.save_no_dups ", "history.save_size ", "history.size "]
        );
        assert_eq!(
            complete(&h, "unsetopt history.s"),
            ["history.save_size ", "history.share ", "history.size "]
        );
        assert_eq!(complete(&h, "setopt history.no"), ["history.no_share "]);
        assert_eq!(complete(&h, "unsetopt history.no"), ["history.no_save_no_dups "]);
        assert_eq!(complete(&h, "setopt errex"), ["errexit "]);
        assert_eq!(complete(&h, "setopt history.file=~/f"), ["history.file=~/file\\ one "]);
        // Commands whose arguments aren't filenames.
        assert_eq!(complete(&h, "cd ~/"), ["~/sub\\ dir/"]);
        // `CDPATH`, after the current directory (the crate's), as in zsh.
        assert_eq!(complete(&h, "cd su"), ["sub\\ dir/"]);
        assert_eq!(complete(&h, "cd sr"), ["src/"]);
        assert_eq!(complete(&h, "pushd 'su"), ["'sub dir/"]);
        assert_eq!(complete(&h, "cd ./su"), Vec::<String>::new());
        assert_eq!(complete(&h, "rmdir su"), Vec::<String>::new());
        assert_eq!(complete(&h, "su"), Vec::<String>::new());
        let mut h = h;
        h.names.autocd = true;
        assert_eq!(complete(&h, "su"), ["sub\\ dir/"]);
        assert_eq!(complete(&h, "sr"), ["src/"]);
        h.names.autocd = false;
        assert_eq!(complete(&h, "unset HO"), ["HOME ", "HOSTNAME "]);
        assert_eq!(complete(&h, "export HOME=~/f"), ["HOME=~/file\\ one "]);
        assert_eq!(complete(&h, "type myf"), ["myfunc "]);
        assert_eq!(complete(&h, "help ech"), ["echo "]);
        assert_eq!(complete(&h, "ll"), ["ll "]);
        assert_eq!(complete(&h, "unalias "), ["ll "]);
        assert_eq!(complete(&h, "unset my"), Vec::<String>::new());
        assert_eq!(complete(&h, "unset -f my"), ["myfunc "]);
        assert_eq!(complete(&h, "unset -f -v HOM"), ["HOME "]);
        assert_eq!(complete(&h, "read -r HOM"), ["HOME "]);
        assert_eq!(complete(&h, "read -p HOM"), Vec::<String>::new());
        assert_eq!(complete(&h, "getopts HOM"), Vec::<String>::new());
        assert_eq!(complete(&h, "getopts ab: HOM"), ["HOME "]);
        assert_eq!(complete(&h, "getopts -- ab: HOM"), ["HOME "]);
        assert_eq!(
            complete(&h, "setopt "),
            [
                "cd.auto ",
                "errexit ",
                "history.file ",
                "history.save_no_dups ",
                "history.save_size ",
                "history.size ",
                "noglob "
            ]
        );
        assert_eq!(complete(&h, "setopt glob"), ["noglob "]);
        assert_eq!(complete(&h, "setopt no"), ["noglob "]);
        assert_eq!(complete(&h, "setopt glob.no"), ["glob.no_star "]);
        assert_eq!(complete(&h, "unsetopt gl"), ["glob.star "]);
        assert_eq!(complete(&h, "unsetopt cd.no_au"), ["cd.no_auto "]);
        assert_eq!(complete(&h, "for HOM"), ["HOME "]);
        assert_eq!(complete(&h, "for x "), ["in "]);
        assert_eq!(complete(&h, "for x in ~/f"), ["~/file\\ one "]);
        // Jobs and signals.
        assert_eq!(complete(&h, "fg "), ["%1 ", "%2 "]);
        assert_eq!(complete(&h, "kill %"), ["%1 ", "%2 "]);
        assert_eq!(complete(&h, "kill -te"), ["-TERM "]);
        assert_eq!(complete(&h, "kill -s KI"), ["KILL "]);
        assert_eq!(complete(&h, "kill -s KILL %2"), ["%2 "]);
        assert_eq!(complete(&h, "trap EX"), Vec::<String>::new());
        assert_eq!(complete(&h, "trap 'echo x' EX"), ["EXIT "]);
        assert_eq!(complete(&h, "trap -- '' in"), ["INT "]);
        assert_eq!(complete(&h, "fg %v"), ["%vi "]);
        assert_eq!(complete(&h, "kill %s"), ["%sleep "]);
        assert_eq!(complete(&h, "kill %2"), ["%2 "]);
        let shown: Vec<_> = (h.complete_bytes(b"wait ", b"").1.into_iter())
            .map(|i| (i.display, i.desc.unwrap_or_default()))
            .collect();
        assert_eq!(
            shown,
            [("%1".into(), "sleep 10 | cat".into()), ("%2".into(), "vi notes".into())]
        );
        // Plugins.
        std::fs::create_dir(dir.join("plugins")).unwrap();
        for f in ["prompt.rhai", "git.rhai", "README"] {
            std::fs::write(dir.join("plugins").join(f), "").unwrap();
        }
        std::fs::create_dir(dir.join("plugins/work")).unwrap();
        assert_eq!(complete(&h, "plugin l"), ["list-available ", "list-loaded ", "load "]);
        assert_eq!(complete(&h, "plugin load "), ["git ", "prompt ", "work "]);
        assert_eq!(complete(&h, "plugin load ~/plugins/p"), ["~/plugins/prompt.rhai "]);
        assert_eq!(complete(&h, "plugin unload "), ["greet "]);
        assert_eq!(complete(&h, "plugin list-loaded "), Vec::<String>::new());
        // Users' home directories.
        assert_eq!(complete(&h, "ls ~roo"), ["~root/"]);
        assert_eq!(complete(&h, "X=a:~roo"), ["X=a:~root/"]);
        assert_eq!(complete(&h, "ls '~roo"), Vec::<String>::new());
        // SAFETY: getpwuid returns a pointer to static storage or null.
        let me = unsafe { std::ffi::CStr::from_ptr((*libc::getpwuid(libc::getuid())).pw_name) };
        let me = me.to_str().unwrap();
        let home = String::from_utf8(sys::home_dir(me.as_bytes()).unwrap()).unwrap();
        let by_path = complete(&h, &format!("ls {home}/"));
        let by_user: Vec<_> = (complete(&h, &format!("ls ~{me}/")).into_iter())
            .map(|c| c.replacen(&format!("~{me}"), &home, 1))
            .collect();
        assert_eq!(by_user, by_path);
        // A plugin's completer.
        assert_eq!(complete(&h, "git "), ["--color=", "add ", "commit "]);
        assert_eq!(complete(&h, "sudo git 'a"), ["'add' "]);
        assert_eq!(complete(&h, "git --c"), ["--color="]);
        assert_eq!(complete(&h, "git add ~/f"), ["~/file\\ one "]);
        assert_eq!(complete(&h, "git add boom"), ["boom"]);
        // Matches ignoring case, then in the middle of the name.
        std::fs::create_dir(dir.join("cases")).unwrap();
        for f in ["Makefile", "README", "my Config.toml", "notes.txt"] {
            std::fs::write(dir.join("cases").join(f), "").unwrap();
        }
        assert_eq!(complete(&h, "ls ~/cases/mak"), ["~/cases/Makefile "]);
        assert_eq!(complete(&h, "ls ~/cases/Rea"), ["~/cases/README "]);
        assert_eq!(complete(&h, "ls ~/cases/MAK"), Vec::<String>::new());
        assert_eq!(complete(&h, "ls ~/cases/conf"), ["~/cases/my\\ Config.toml "]);
        assert_eq!(
            complete(&h, "ls ~/cases/t"),
            ["~/cases/my\\ Config.toml ", "~/cases/notes.txt "]
        );
        assert_eq!(complete(&h, "ls ~/cases/'conf"), ["~/cases/'my Config.toml' "]);
        assert_eq!(complete(&h, "ls ~/cases/\"my c"), ["~/cases/\"my Config.toml\" "]);
        assert_eq!(complete(&h, "ls ~/cases/my\\ c"), ["~/cases/my\\ Config.toml "]);
        assert_eq!(complete(&h, "X=~/cases/'conf"), ["X=~/cases/'my Config.toml' "]);
        assert_eq!(complete(&h, "echo $home"), ["HOME"]);
        assert_eq!(complete(&h, "echo \"${home"), ["HOME}"]);
        assert_eq!(complete(&h, "git 'Com"), Vec::<String>::new());
        assert_eq!(complete(&h, "git 'mit"), ["'commit' "]);
        // Through an alias, and with words after the cursor.
        h.names.aliases.push((b"g".to_vec(), b"git".to_vec()));
        assert_eq!(complete(&h, "g c"), ["commit "]);
        assert_eq!(h.complete_bytes(b"git ", b" x after").1.len(), 0);
        // Candidates that complete the part after a prefix.
        assert_eq!(complete(&h, "git log --pretty=o"), ["--pretty=oneline "]);
        assert_eq!(complete(&h, "git log '--pretty=s"), ["'--pretty=short' "]);
        assert_eq!(
            complete(&h, "git log --pretty="),
            ["--pretty=oneline ", "--pretty=short "]
        );
        // The default completer, for commands without one of their own
        // whose arguments luish doesn't know.
        assert_eq!(complete(&h, "frob xyl"), Vec::<String>::new());
        h.names.completers.push(DEFAULT_COMPLETER.to_vec());
        assert_eq!(complete(&h, "frob xyl"), ["xylophone "]);
        assert_eq!(complete(&h, "git c"), ["commit "]);
        assert_eq!(complete(&h, "cd ~/s"), ["~/sub\\ dir/"]);
        assert_eq!(complete(&h, "ls ~/f"), ["~/file\\ one "]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn menu_items() {
        let c = |v: &str, d: Option<&str>| Candidate {
            desc: d.map(|d| d.as_bytes().to_vec()),
            ..Candidate::word(v.as_bytes())
        };
        let w = analyze(b"", &[]);
        let t = Target {
            w: &w,
            line: b"",
            from: 0,
            quote: Quote::None,
        };
        let it = items(
            &[c("commit", Some("Record\nchanges")), c("add", None), c("add", None)],
            &t,
        );
        let item = |d: &str, desc: Option<&str>, r: &str| Item {
            display: d.into(),
            desc: desc.map(Into::into),
            replacement: r.into(),
        };
        assert_eq!(
            it,
            [
                item("add", None, "add "),
                item("commit", Some("Record?changes"), "commit ")
            ]
        );
        assert_eq!(common_prefix(&it), "");
        let it = [item("", None, "h\u{e9}t "), item("", None, "h\u{e8}t ")];
        assert_eq!(common_prefix(&it), "h");
    }

    /// Tab, as rustyline handles it: the one candidate replaces the text
    /// from where it starts to the cursor.
    fn tab(h: &ShellHelper, line: &mut String) {
        let history = super::super::history::ShellHistory::default();
        let (start, c) = h.complete(line, line.len(), &Context::new(&history)).unwrap();
        if let Some(c) = c.first() {
            line.replace_range(start.., &c.replacement);
        }
    }

    fn menu_shown(h: &ShellHelper, line: &str) -> Option<String> {
        let history = super::super::history::ShellHistory::default();
        h.hint(line, line.len(), &Context::new(&history)).map(|d| d.display)
    }

    #[test]
    fn tab_opens_the_menu() {
        let h = ShellHelper {
            names: Names {
                functions: vec![b"myfunc_a".to_vec(), b"myfunc_b".to_vec(), b"other".to_vec()],
                path: b"/nonexistent".to_vec(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut line = "myf".to_string();
        tab(&h, &mut line);
        assert_eq!((&line[..], menu_shown(&h, &line)), ("myfunc_", None));
        // Nothing more in common: the menu is shown, nothing selected.
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_");
        assert_eq!(menu_shown(&h, &line).as_deref(), Some("\nmyfunc_a  myfunc_b"));
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_a ");
        assert_eq!(
            menu_shown(&h, &line).as_deref(),
            Some("\n\x1b[7mmyfunc_a\x1b[0m  myfunc_b")
        );
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_b ");
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_a ");
        // A key binding's move: Esc puts back the text typed.
        h.menu.lock().unwrap().pending = Some(menu::Move::Cancel);
        tab(&h, &mut line);
        assert_eq!((&line[..], menu_shown(&h, &line)), ("myfunc_", None));
        // Once the line changes, the menu is closed.
        tab(&h, &mut line);
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_a ");
        line.push_str("/nonexistent/x");
        assert_eq!(menu_shown(&h, &line), None);
        tab(&h, &mut line);
        assert_eq!(line, "myfunc_a /nonexistent/x");
    }
}
