//! Tab completion for the line editor.
//!
//! The completer (and the highlighter, in `highlight.rs`) never touches `Shell`: before each prompt the REPL gives it
//! a snapshot of the names it needs (`Names`). It completes command names in
//! command position (built-ins, reserved words, functions, aliases and
//! `PATH`), variable names after `$` and `${`, and filenames elsewhere.
//! Completions keep the text already typed and quote what they add, according
//! to the quoting in effect at the cursor.

use std::cell::RefCell;
use std::os::unix::ffi::OsStrExt;

use rustyline::completion::{Completer, Pair};
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use crate::path::{DirStamp, dir_stamps};
use crate::sys;

/// What the completer knows about the shell, refreshed before each prompt.
#[derive(Default)]
pub struct Names {
    /// Functions and aliases (built-ins and reserved words are added here).
    pub commands: Vec<Vec<u8>>,
    pub vars: Vec<Vec<u8>>,
    pub path: Vec<u8>,
    pub home: Option<Vec<u8>>,
}

#[derive(Default)]
pub struct ShellHelper {
    pub names: Names,
    pub highlight: super::highlight::State,
    path_cache: RefCell<PathCache>,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Quote {
    None,
    Single,
    Double,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Command,
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
    /// The unquoted text to complete.
    text: Vec<u8>,
}

/// The tokenizer's state while scanning the line up to the cursor.
struct Scan {
    /// Whether the next word is a command name.
    cmd_pos: bool,
    /// Whether the next word is the target of a redirection.
    redirect: bool,
    /// Whether the last command word takes a command as its argument.
    precommand: bool,
    /// The state outside each open `$(`, `(` or backquote: its quoting, and
    /// whether it was opened by a backquote.
    stack: Vec<(Quote, bool)>,
    /// Start of the current word, if in one.
    start: Option<usize>,
    /// The current word, unquoted.
    text: Vec<u8>,
    /// Where in `text` the part to complete begins (after `=` or `:`).
    split: usize,
    quote: Quote,
}

impl Scan {
    /// Ends the current word, updating the command-position state.
    fn end_word(&mut self) {
        if self.start.take().is_none() {
            return;
        }
        let w = &self.text[..];
        if self.redirect {
            self.redirect = false;
        } else if !self.cmd_pos || is_assignment(w) || (self.precommand && w.starts_with(b"-")) {
            // no change
        } else if BEFORE_COMMAND.contains(&w) {
            self.precommand = false;
        } else if PRECOMMANDS.contains(&w) {
            self.precommand = true;
        } else {
            self.cmd_pos = false;
            self.precommand = false;
        }
        self.text.clear();
    }

    /// Starts a nested command (`$(`, `(` or a backquote).
    fn open(&mut self, backquote: bool) {
        self.end_word();
        self.stack.push((self.quote, backquote));
        self.quote = Quote::None;
        self.cmd_pos = true;
        self.precommand = false;
        self.redirect = false;
    }

    /// Ends a nested command. The rest of a word containing a substitution
    /// can't be completed, so it is treated as a new argument.
    fn close(&mut self) {
        self.end_word();
        if let Some((q, _)) = self.stack.pop() {
            self.quote = q;
        }
        self.cmd_pos = false;
    }

    fn in_backquote(&self) -> bool {
        self.stack.last().is_some_and(|s| s.1)
    }
}

/// Finds the word ending at the end of `line` and what it should complete to.
/// This is a rough tokenizer: it follows quoting, operators, redirections,
/// assignments, and substitutions, but not the full grammar.
fn analyze(line: &[u8]) -> Word {
    let mut s = Scan {
        cmd_pos: true,
        redirect: false,
        precommand: false,
        stack: Vec::new(),
        start: None,
        text: Vec::new(),
        split: 0,
        quote: Quote::None,
    };
    let mut i = 0;
    while i < line.len() {
        let c = line[i];
        let next = line.get(i + 1).copied();
        i += 1;
        match (s.quote, c) {
            (Quote::Single, b'\'') | (Quote::Double, b'"') => s.quote = Quote::None,
            (Quote::Single, _) => s.text.push(c),
            (Quote::Double | Quote::None, b'$') if next == Some(b'(') => {
                s.open(false);
                i += 1;
            }
            (Quote::Double | Quote::None, b'`') => {
                if s.in_backquote() {
                    s.close();
                } else {
                    s.open(true);
                }
            }
            (Quote::Double, b'\\') if next.is_some_and(|n| b"$`\"\\\n".contains(&n)) => {
                s.text.push(line[i]);
                i += 1;
            }
            (Quote::Double, _) => s.text.push(c),
            (Quote::None, b' ' | b'\t') => s.end_word(),
            (Quote::None, b'\n' | b';' | b'&' | b'|') => {
                s.end_word();
                s.cmd_pos = true;
                s.precommand = false;
                s.redirect = false;
            }
            (Quote::None, b'(') => s.open(false),
            (Quote::None, b')') => s.close(),
            (Quote::None, b'<' | b'>') => {
                // A word of digits before the operator is an fd number.
                if !s.text.iter().all(u8::is_ascii_digit) {
                    s.end_word();
                }
                s.start = None;
                s.text.clear();
                while i < line.len() && b"<>&|-".contains(&line[i]) {
                    i += 1;
                }
                s.redirect = true;
            }
            (Quote::None, b'#') if s.start.is_none() => {
                return Word {
                    start: line.len(),
                    kind: Kind::Nothing,
                    quote: Quote::None,
                    text: Vec::new(),
                };
            }
            (Quote::None, _) => {
                if s.start.is_none() {
                    s.start = Some(i - 1);
                    s.split = 0;
                }
                match c {
                    b'\\' => {
                        if let Some(n) = next {
                            s.text.push(n);
                            i += 1;
                        }
                    }
                    b'\'' => s.quote = Quote::Single,
                    b'"' => s.quote = Quote::Double,
                    _ => {
                        s.text.push(c);
                        let assignment = s.cmd_pos && !s.redirect && is_assignment(&s.text);
                        if (c == b'=' && (assignment || s.text.starts_with(b"--"))) || (c == b':' && assignment) {
                            s.split = s.text.len();
                        }
                    }
                }
            }
        }
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
                };
            }
        }
    }

    let kind = if !s.redirect && s.cmd_pos && !is_assignment(&s.text) {
        Kind::Command
    } else {
        Kind::File
    };
    let text = if kind == Kind::File {
        s.text[s.split..].to_vec()
    } else {
        s.text
    };
    Word {
        start: s.start.unwrap_or(line.len()),
        kind,
        quote: s.quote,
        text,
    }
}

/// Whether `w` starts with `NAME=`.
fn is_assignment(w: &[u8]) -> bool {
    match w.iter().position(|&c| c == b'=') {
        Some(eq) => crate::lexer::is_valid_name(&w[..eq]),
        None => false,
    }
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

/// Builds a candidate: `typed` (the raw text already in the line) followed by
/// the quoted rest of `name` and `end`.
fn candidate(display: &[u8], typed: &[u8], rest: &[u8], quote: Quote, end: &[u8]) -> Option<Pair> {
    let mut r = typed.to_vec();
    quote_suffix(rest, quote, typed.is_empty(), &mut r);
    r.extend_from_slice(end);
    Some(Pair {
        display: String::from_utf8(display.to_vec()).ok()?,
        replacement: String::from_utf8(r).ok()?,
    })
}

pub(super) fn is_executable(path: &[u8]) -> bool {
    sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFREG) && sys::access(path, libc::X_OK)
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

impl ShellHelper {
    fn complete_bytes(&self, line: &[u8]) -> (usize, Vec<Pair>) {
        let w = analyze(line);
        let typed = &line[w.start..];
        let mut out = Vec::new();
        match w.kind {
            Kind::Nothing => {}
            Kind::Var(brace) => {
                for v in &self.names.vars {
                    if let Some(rest) = v.strip_prefix(&w.text[..]) {
                        out.extend(candidate(v, typed, rest, Quote::None, if brace { b"}" } else { b"" }));
                    }
                }
            }
            Kind::Command if !w.text.contains(&b'/') => {
                let mut cache = self.path_cache.borrow_mut();
                cache.refresh(&self.names.path);
                // (The `map` shortens the built-in names' `'static` lifetime.)
                let all = crate::builtins::names()
                    .map(|b: &[u8]| b)
                    .chain(RESERVED.iter().copied())
                    .chain(self.names.commands.iter().map(|c| &c[..]))
                    .chain(cache.names.iter().map(|c| &c[..]));
                let end = [closing(w.quote), b" "].concat();
                for name in all {
                    if let Some(rest) = name.strip_prefix(&w.text[..]) {
                        out.extend(candidate(name, typed, rest, w.quote, &end));
                    }
                }
            }
            Kind::Command | Kind::File => self.complete_file(&w, typed, w.kind == Kind::Command, &mut out),
        }
        out.sort_unstable_by(|a, b| a.replacement.cmp(&b.replacement));
        out.dedup_by(|a, b| a.replacement == b.replacement);
        (w.start, out)
    }

    fn complete_file(&self, w: &Word, typed: &[u8], executables: bool, out: &mut Vec<Pair>) {
        let slash = w.text.iter().rposition(|&c| c == b'/').map_or(0, |i| i + 1);
        let (dir, prefix) = w.text.split_at(slash);
        let mut dir = dir.to_vec();
        if dir.starts_with(b"~/")
            && let Some(home) = &self.names.home
        {
            dir.splice(..1, home.iter().copied());
        }
        let mut names = read_dir(&dir);
        if prefix == b".." {
            names.push(b"..".to_vec());
        }
        for name in names {
            let Some(rest) = name.strip_prefix(prefix) else {
                continue;
            };
            if name.starts_with(b".") && !prefix.starts_with(b".") {
                continue;
            }
            let mut full = if dir.is_empty() { b"./".to_vec() } else { dir.clone() };
            full.extend_from_slice(&name);
            let is_dir = sys::stat(&full).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFDIR);
            if is_dir {
                let display = [&name[..], b"/"].concat();
                out.extend(candidate(&display, typed, rest, w.quote, b"/"));
            } else if !executables || is_executable(&full) {
                let end = [closing(w.quote), b" "].concat();
                out.extend(candidate(&name, typed, rest, w.quote, &end));
            }
        }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        Ok(self.complete_bytes(&line.as_bytes()[..pos]))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}
impl Validator for ShellHelper {}
impl Helper for ShellHelper {}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(line: &str) -> (Kind, String) {
        let w = analyze(line.as_bytes());
        (w.kind, String::from_utf8(w.text).unwrap())
    }

    #[test]
    fn command_position() {
        use Kind::*;
        assert_eq!(kind("ec"), (Command, "ec".into()));
        assert_eq!(kind("echo hi; ca"), (Command, "ca".into()));
        assert_eq!(kind("echo ca"), (File, "ca".into()));
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
        assert_eq!(kind("x >&2 fo"), (File, "fo".into()));
        assert_eq!(kind(">out ca"), (Command, "ca".into()));
        assert_eq!(kind("./a"), (Command, "./a".into()));
        assert_eq!(kind("echo # co"), (Nothing, "".into()));
    }

    #[test]
    fn quoting_and_splitting() {
        use Kind::*;
        assert_eq!(kind("ls 'a b"), (File, "a b".into()));
        assert_eq!(kind("ls a\\ b"), (File, "a b".into()));
        assert_eq!(kind("ls \"a\\$b"), (File, "a$b".into()));
        assert_eq!(kind("PATH=/bin:/usr/b"), (File, "/usr/b".into()));
        assert_eq!(kind("ls --file=fo"), (File, "fo".into()));
        assert_eq!(kind("ls a=b"), (File, "a=b".into()));
        assert_eq!(kind("echo $HO"), (Var(false), "HO".into()));
        assert_eq!(kind("echo \"${HO"), (Var(true), "HO".into()));
        assert_eq!(kind("echo '$HO"), (File, "$HO".into()));
        assert_eq!(kind("echo \\$HO"), (File, "$HO".into()));
        let w = analyze(b"ls 'a b");
        assert_eq!((w.start, w.quote), (3, Quote::Single));
    }

    fn complete(h: &ShellHelper, line: &str) -> Vec<String> {
        h.complete_bytes(line.as_bytes())
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
                commands: vec![b"myfunc".to_vec()],
                vars: vec![b"HOME".to_vec(), b"HOSTNAME".to_vec()],
                path: format!("{d}/sub dir").into_bytes(),
                home: Some(dir.as_os_str().as_bytes().to_vec()),
            },
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
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
