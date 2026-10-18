//! `basename` and `dirname` run in the shell, without a process, when they
//! are the system's (`PATH` finds them in `/usr/bin` or `/bin`) and the
//! arguments are ones whose result POSIX specifies. Anything else (options,
//! the cases POSIX leaves to the implementation, a `basename` of one's own
//! earlier in `PATH`) runs the utility itself, so nothing changes but the
//! time taken: `type` and `command -v` still show the file.
//!
//! `$(basename ...)` and `$(dirname ...)` are also done without a subshell
//! when nothing in the words can have a side effect or fail
//! ([`utility_call`]): the result is then what the subshell would print.

use super::simple::CommandKind;
use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;

fn is_utility(name: &[u8]) -> bool {
    name == b"basename" || name == b"dirname"
}

/// What `argv` prints (without the newline), or `None` if the utility
/// has to decide: an option (GNU's `getopt` takes one after an operand
/// too), a wrong number of operands, or a result up to the implementation.
fn output(argv: &[Vec<u8>]) -> Option<Vec<u8>> {
    let (name, mut args) = argv.split_first()?;
    if args.first().is_some_and(|a| a == b"--") {
        args = &args[1..];
    } else if args.iter().any(|a| a.starts_with(b"-")) {
        return None;
    }
    match (&name[..], args) {
        (b"basename", [s]) => basename(s, b""),
        (b"basename", [s, suffix]) => basename(s, suffix),
        (b"dirname", [s]) => dirname(s),
        _ => None,
    }
}

fn trim_slashes(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&c| c != b'/').map_or(0, |i| i + 1);
    &s[..end]
}

/// POSIX's `basename`. The result for an empty string and for `//` is up
/// to the implementation.
fn basename(s: &[u8], suffix: &[u8]) -> Option<Vec<u8>> {
    if s.is_empty() || s == b"//" {
        return None;
    }
    let s = trim_slashes(s);
    if s.is_empty() {
        return Some(b"/".to_vec());
    }
    let base = s.iter().rposition(|&c| c == b'/').map_or(s, |i| &s[i + 1..]);
    Some(match base.strip_suffix(suffix) {
        Some(rest) if !rest.is_empty() => rest.to_vec(),
        _ => base.to_vec(),
    })
}

/// POSIX's `dirname`. The result for `//`, and for `//name` (whose
/// directory is `//`), is up to the implementation (busybox keeps the `//`,
/// coreutils doesn't), and an empty string is unclear.
fn dirname(s: &[u8]) -> Option<Vec<u8>> {
    if s.is_empty() || (s.starts_with(b"//") && s.get(2) != Some(&b'/')) {
        return None;
    }
    let s = trim_slashes(s);
    let Some(i) = s.iter().rposition(|&c| c == b'/') else {
        return Some(if s.is_empty() { b"/" } else { b"." }.to_vec());
    };
    let dir = trim_slashes(&s[..i]);
    Some(if dir.is_empty() { b"/" } else { dir }.to_vec())
}

/// The command of a `$(...)` that can run without a subshell: a lone
/// `basename` or `dirname` with neither assignments nor redirections, whose
/// words expand without side effects.
fn utility_call(list: &List) -> Option<&SimpleCommand> {
    let [cc] = &list[..] else { return None };
    let pipeline = &cc.list.first;
    if cc.async_ || !cc.list.rest.is_empty() || pipeline.negated {
        return None;
    }
    let [Command::Simple(sc)] = &pipeline.cmds[..] else {
        return None;
    };
    let (first, rest) = sc.words.split_first()?;
    let [WordPart::Literal(name)] = &first.0[..] else {
        return None;
    };
    let simple = is_utility(name) && sc.assigns.is_empty() && sc.redirs.is_empty();
    (simple && rest.iter().all(|w| pure(&w.0))).then_some(sc)
}

/// Whether expanding `parts` has no side effects, and can't fail unless
/// `set -u` or a `glob.fail` option is on. `$RANDOM` changes the generator,
/// and `$LINENO` is the line of the substitution's command in a subshell.
fn pure(parts: &[WordPart]) -> bool {
    parts.iter().all(|p| match p {
        WordPart::Literal(_) | WordPart::SingleQuoted(_) | WordPart::Escaped(_) | WordPart::Tilde(_) => true,
        WordPart::DoubleQuoted(inner) => pure(inner),
        WordPart::Param(pe) => {
            let name = match &pe.name {
                ParamName::Var(n) => n != b"RANDOM" && n != b"LINENO",
                ParamName::Positional(_) | ParamName::Special(_) => true,
                ParamName::Indirect(_) => false,
            };
            let op = match &pe.op {
                ParamOp::Plain | ParamOp::Length => true,
                ParamOp::RemoveSmallestSuffix(w)
                | ParamOp::RemoveLargestSuffix(w)
                | ParamOp::RemoveSmallestPrefix(w)
                | ParamOp::RemoveLargestPrefix(w) => pure(&w.0),
                _ => false,
            };
            name && op && pe.index.is_none() && pe.flags.is_none()
        }
        _ => false,
    })
}

impl Shell {
    /// What `argv` prints if it is `basename` or `dirname` run in the shell
    /// (see the module's documentation). `remember` records the command in
    /// the cache (`hash`), as running it would; a subshell's lookup doesn't.
    fn utility_output(&mut self, argv: &[Vec<u8>], remember: bool) -> Option<Vec<u8>> {
        let name = &argv[0];
        let out = output(argv)?;
        let path = match self.hash.get(name) {
            Some((p, _)) => p.clone(),
            None if remember => self.find_in_path(name)?.0,
            None => match crate::path::search(&self.get_var(b"PATH").unwrap_or_default(), name)? {
                (p, _, true) => p,
                _ => return None,
            },
        };
        let dir = path.strip_suffix(&name[..])?;
        (dir == b"/usr/bin/" || dir == b"/bin/").then_some(out)
    }

    /// Runs `argv`, an external command, in the shell if it is `basename`
    /// or `dirname` (see the module's documentation), and returns its
    /// status. Not when standard output is a pipe: if nothing reads it, it
    /// is the utility that SIGPIPE should kill, not the shell.
    pub fn run_utility(&mut self, argv: &[Vec<u8>]) -> Option<i32> {
        if !is_utility(&argv[0]) {
            return None;
        }
        let kind = sys::fstat(1)?.st_mode & libc::S_IFMT;
        if kind == libc::S_IFIFO || kind == libc::S_IFSOCK {
            return None;
        }
        let mut out = self.utility_output(argv, true)?;
        out.push(b'\n');
        if sys::write_all(1, &out) {
            return Some(0);
        }
        self.berr(&argv[0], "write error");
        Some(1)
    }

    /// The value of `$(list)` if it is `basename` or `dirname` that can run
    /// without a subshell (see [`utility_call`]).
    #[inline(never)]
    pub fn subst_utility(&mut self, list: &List) -> Result<Option<Vec<u8>>, Flow> {
        let Some(sc) = utility_call(list) else {
            return Ok(None);
        };
        let opts = [Opt::Xtrace, Opt::Nounset, Opt::GlobFail, Opt::GlobFailAll];
        // A subshell's exit runs a CHLD trap.
        let chld = self.traps[libc::SIGCHLD as usize]
            .as_ref()
            .is_some_and(|a| !a.is_empty());
        if chld || opts.into_iter().any(|o| self.opt(o)) {
            return Ok(None);
        }
        let WordPart::Literal(name) = &sc.words[0].0[0] else {
            return Ok(None);
        };
        if !matches!(self.lookup_command(name, true), CommandKind::External) {
            return Ok(None);
        }
        let argv = self.expand_command_words(&sc.words)?;
        let Some(mut out) = self.utility_output(&argv, false) else {
            return Ok(None);
        };
        while out.last() == Some(&b'\n') {
            out.pop();
        }
        self.subst_status = Some(0);
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Option<String> {
        let argv: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes().to_vec()).collect();
        output(&argv).map(|o| String::from_utf8(o).unwrap())
    }

    #[test]
    fn basename() {
        for (s, want) in [
            ("/", "/"),
            ("///", "/"),
            ("a", "a"),
            ("a//", "a"),
            ("/a/b/", "b"),
            ("//a", "a"),
            ("a//b//", "b"),
            (".", "."),
            ("../x", "x"),
        ] {
            assert_eq!(run(&["basename", s]).as_deref(), Some(want), "{s}");
        }
        assert_eq!(run(&["basename", "a/b.txt", ".txt"]).as_deref(), Some("b"));
        assert_eq!(run(&["basename", "a/.txt", ".txt"]).as_deref(), Some(".txt"));
        assert_eq!(run(&["basename", "a.txt/", ".txt"]).as_deref(), Some("a"));
        assert_eq!(run(&["basename", "x/", "x"]).as_deref(), Some("x"));
        assert_eq!(run(&["basename", "--", "a/b"]).as_deref(), Some("b"));
        assert_eq!(run(&["basename", "--", "-a"]).as_deref(), Some("-a"));
        for args in [
            &["basename", ""][..],
            &["basename", "//"],
            &["basename"],
            &["basename", "a", "b", "c"],
            &["basename", "-a", "x"],
            &["basename", "x", "-s"],
            &["basename", "x", "--"],
        ] {
            assert_eq!(run(args), None, "{args:?}");
        }
    }

    #[test]
    fn dirname() {
        for (s, want) in [
            ("/", "/"),
            ("///", "/"),
            ("a", "."),
            ("a/", "."),
            ("/a", "/"),
            ("/a/", "/"),
            ("a/b", "a"),
            ("a//b//", "a"),
            ("/a/b/c.txt", "/a/b"),
            ("///a", "/"),
            ("..", "."),
            ("../x", ".."),
        ] {
            assert_eq!(run(&["dirname", s]).as_deref(), Some(want), "{s}");
        }
        for args in [
            &["dirname", ""][..],
            &["dirname", "//"],
            &["dirname", "//a"],
            &["dirname", "a", "b"],
        ] {
            assert_eq!(run(args), None, "{args:?}");
        }
    }
}
