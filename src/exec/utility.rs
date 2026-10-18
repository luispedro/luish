//! `basename`, `dirname`, `expr`, `seq`, `id` and `uname` run in the shell,
//! without a process, when they are the system's (`PATH` finds them in
//! `/usr/bin` or `/bin`) and the arguments are ones whose result POSIX
//! specifies (for `expr`, arithmetic and comparisons of integers), and on
//! which coreutils, uutils and busybox agree. Anything else (options, the
//! cases POSIX leaves to the implementation, a `basename` of one's own
//! earlier in `PATH`) runs the utility itself, so nothing changes but the
//! time taken: `type` and `command -v` still show the file.
//!
//! `$(...)` runs without a subshell when it is one of those utilities or a
//! built-in that only prints (`echo`, `printf`, `pwd`, `command -v`,
//! `type`), with no redirection but `2>/dev/null`, and nothing in the words
//! can have a side effect or fail
//! ([`in_place_call`]): the result is then what the subshell would print.
//! A utility's words may also hold a `$(...)` (`$(basename "$(dirname
//! "$f")")`), which can't change the shell either, as it runs in place
//! itself or in a subshell of its own. Such words are expanded only once:
//! if the utility has to run after all, the subshell gets them expanded
//! ([`Subst::Expanded`]).

use super::simple::CommandKind;
use crate::ast::*;
use crate::builtins::BuiltinFn;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;

fn is_utility(name: &[u8]) -> bool {
    matches!(name, b"basename" | b"dirname" | b"expr" | b"seq" | b"id" | b"uname")
}

/// What `argv` prints and its status, or `None` if
/// the utility has to decide: an option (GNU's `getopt` takes one after an
/// operand too), a wrong number of operands, or a result up to the
/// implementation.
fn output(argv: &[Vec<u8>]) -> Option<(Vec<u8>, i32)> {
    let (name, mut args) = argv.split_first()?;
    match &name[..] {
        b"expr" => return expr(args),
        b"seq" => return seq(args),
        b"id" => return id(args).map(|o| (o, 0)),
        b"uname" => return uname(args).map(|o| (o, 0)),
        _ => {}
    }
    if args.first().is_some_and(|a| a == b"--") {
        args = &args[1..];
    } else if args.iter().any(|a| a.starts_with(b"-")) {
        return None;
    }
    let out = match (&name[..], args) {
        (b"basename", [s]) => basename(s, b""),
        (b"basename", [s, suffix]) => basename(s, suffix),
        (b"dirname", [s]) => dirname(s),
        _ => None,
    };
    out.map(|mut o| {
        o.push(b'\n');
        (o, 0)
    })
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

/// `expr` with integers only: `-?[0-9]+` operands, parentheses, `*`, `/`,
/// `%`, `+`, `-` and the comparisons, by POSIX's precedence (all left
/// associative). Anything else is the utility's: strings (compared by
/// collation), `:`, `|` and `&` (whose values are strings), GNU's
/// keywords, `+5` (an integer to some), a first argument starting with `-`
/// (busybox rejects `--`), overflow (coreutils uses big numbers), division
/// by zero, and a lone operand, which is printed as it is (`007`).
fn expr(args: &[Vec<u8>]) -> Option<(Vec<u8>, i32)> {
    if args.first()?.starts_with(b"-") {
        return None;
    }
    let mut e = Expr {
        args,
        pos: 0,
        applied: false,
    };
    let v = e.compare()?;
    (e.pos == args.len() && e.applied).then(|| (format!("{v}\n").into_bytes(), (v == 0) as i32))
}

/// `seq LAST`, `seq FIRST LAST` and `seq FIRST INCREMENT LAST`, with
/// integers below 10^15 in magnitude (busybox counts in doubles) and at
/// most [`SEQ_MAX`] lines. The utility runs for anything else: options
/// (`--` included), `+1`, `-0` (which coreutils prints as `-0`), an
/// increment of zero (an error in coreutils, an endless loop in busybox),
/// fractions and exponents.
fn seq(args: &[Vec<u8>]) -> Option<(Vec<u8>, i32)> {
    let n: Vec<i64> = args.iter().map(|a| seq_number(a)).collect::<Option<_>>()?;
    let (first, step, last) = match n[..] {
        [last] => (1, 1, last),
        [first, last] => (first, 1, last),
        [first, step, last] => (first, step, last),
        _ => return None,
    };
    if step == 0 {
        return None;
    }
    let count = if (last - first).signum() == -step.signum() {
        0
    } else {
        (last - first) / step + 1
    };
    if count > SEQ_MAX {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..count {
        out.extend_from_slice(format!("{}\n", first + i * step).as_bytes());
    }
    Some((out, 0))
}

/// The most lines `seq` prints in the shell: past that, the utility's own
/// time counts for more than starting it, and a loop in the shell couldn't
/// be interrupted.
const SEQ_MAX: i64 = 100_000;

fn seq_number(a: &[u8]) -> Option<i64> {
    let digits = a.strip_prefix(b"-").unwrap_or(a);
    if digits.is_empty() || digits.len() > 15 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n: i64 = std::str::from_utf8(a).ok()?.parse().ok()?;
    (n != 0 || a[0] != b'-').then_some(n)
}

struct Expr<'a> {
    args: &'a [Vec<u8>],
    pos: usize,
    /// Whether an operator was applied, so the value is a number.
    applied: bool,
}

impl Expr<'_> {
    /// Takes the next argument if it is one of `ops`.
    fn op(&mut self, ops: &[&'static [u8]]) -> Option<&'static [u8]> {
        let a = self.args.get(self.pos)?;
        let op = *ops.iter().find(|o| **o == &a[..])?;
        self.pos += 1;
        self.applied = true;
        Some(op)
    }

    fn compare(&mut self) -> Option<i64> {
        let mut v = self.sum()?;
        while let Some(op) = self.op(&[b"=", b"!=", b"<", b"<=", b">", b">="]) {
            let w = self.sum()?;
            v = i64::from(match op {
                b"=" => v == w,
                b"!=" => v != w,
                b"<" => v < w,
                b"<=" => v <= w,
                b">" => v > w,
                _ => v >= w,
            });
        }
        Some(v)
    }

    fn sum(&mut self) -> Option<i64> {
        let mut v = self.product()?;
        while let Some(op) = self.op(&[b"+", b"-"]) {
            let w = self.product()?;
            v = if op == b"+" { v.checked_add(w) } else { v.checked_sub(w) }?;
        }
        Some(v)
    }

    fn product(&mut self) -> Option<i64> {
        let mut v = self.primary()?;
        while let Some(op) = self.op(&[b"*", b"/", b"%"]) {
            let w = self.primary()?;
            v = match op {
                b"*" => v.checked_mul(w),
                b"/" => v.checked_div(w),
                _ => v.checked_rem(w),
            }?;
        }
        Some(v)
    }

    fn primary(&mut self) -> Option<i64> {
        let a = self.args.get(self.pos)?;
        self.pos += 1;
        if a == b"(" {
            let v = self.compare()?;
            if self.args.get(self.pos)? != b")" {
                return None;
            }
            self.pos += 1;
            return Some(v);
        }
        let digits = a.strip_prefix(b"-").unwrap_or(a);
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(a).ok()?.parse().ok()
    }
}

/// The option letters of `args`, each argument being `-` and letters from
/// `allowed`, or `None` (an operand, `--`, a long option, another letter).
fn option_letters(args: &[Vec<u8>], allowed: &[u8]) -> Option<Vec<u8>> {
    let mut letters = Vec::new();
    for a in args {
        let l = a.strip_prefix(b"-").filter(|l| !l.is_empty())?;
        if !l.iter().all(|c| allowed.contains(c)) {
            return None;
        }
        letters.extend_from_slice(l);
    }
    Some(letters)
}

/// `id -u` or `id -g`, with `-n` (the name) or `-r` (the real id). Not
/// without either (the groups' order differs between implementations), nor
/// with a user, nor for an id without a name (an error).
fn id(args: &[Vec<u8>]) -> Option<Vec<u8>> {
    let letters = option_letters(args, b"ugnr")?;
    let has = |c| letters.contains(&c);
    let real = has(b'r');
    let mut out = match (has(b'u'), has(b'g'), has(b'n')) {
        (true, false, false) => if real { sys::getuid() } else { sys::geteuid() }
            .to_string()
            .into_bytes(),
        (false, true, false) => if real { sys::getgid() } else { sys::getegid() }
            .to_string()
            .into_bytes(),
        (true, false, true) => sys::user_name_of(if real { sys::getuid() } else { sys::geteuid() })?,
        (false, true, true) => sys::group_name_of(if real { sys::getgid() } else { sys::getegid() })?,
        _ => return None,
    };
    out.push(b'\n');
    Some(out)
}

/// `uname` with any of `-s`, `-n`, `-r`, `-v` and `-m` (`-s` without
/// any), each once (uutils rejects a repeated one), printed in that order.
/// Not `-o`, `-p`, `-i` or `-a`, whose output differs between
/// implementations.
fn uname(args: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut letters = option_letters(args, b"snrvm")?;
    let n = letters.len();
    letters.sort_unstable();
    letters.dedup();
    if letters.len() != n {
        return None;
    }
    if letters.is_empty() {
        letters.push(b's');
    }
    let fields = sys::uname()?;
    let mut out = Vec::new();
    for (c, field) in b"snrvm".iter().zip(fields) {
        if letters.contains(c) {
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend(field);
        }
    }
    out.push(b'\n');
    Some(out)
}

/// The built-ins that `$(...)` can run without a subshell, as they change
/// nothing in the shell. `command` only with `-v` or `-V`, and it and `type`
/// add to the command cache (`hash`), which is undone.
const PRINTERS: &[&[u8]] = &[b"echo", b"printf", b"pwd", b"command", b"type"];

/// The command of a `$(...)` that can run without a subshell: a lone
/// utility or built-in that only prints, with no assignments and no
/// redirection but `2>/dev/null` (which a utility run in the shell doesn't
/// need), whose words expand without side effects but for a utility's
/// `$(...)`. The `bool` says whether there is one.
fn in_place_call(list: &List) -> Option<(&SimpleCommand, &[u8], bool)> {
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
    let utility = is_utility(name);
    let known = utility || PRINTERS.contains(&&name[..]);
    if !known || !sc.assigns.is_empty() || !sc.redirs.iter().all(quiet) {
        return None;
    }
    let mut nested = false;
    let mut substs = utility.then_some(&mut nested);
    let pure = rest.iter().all(|w| pure(&w.0, &mut substs));
    pure.then_some((sc, name, nested))
}

/// The list of `$(sc)` with `argv`, its words expanded, quoted instead.
fn expanded(sc: &SimpleCommand, argv: Vec<Vec<u8>>) -> List {
    let words = argv
        .into_iter()
        .map(|a| Word(Parts::One(WordPart::SingleQuoted(a))))
        .collect();
    let sc = SimpleCommand { words, ..sc.clone() };
    let pipeline = Pipeline {
        negated: false,
        cmds: vec![Command::Simple(sc)],
    };
    let list = AndOrList {
        first: pipeline,
        rest: Vec::new(),
    };
    vec![CompleteCommand {
        list,
        async_: false,
        disown: false,
    }]
}

/// Whether a redirection is `2>/dev/null`, which can't fail or have side
/// effects (`$(command -v x 2>/dev/null)` is common).
fn quiet(r: &Redirect) -> bool {
    let RedirTarget::Word(Word(target)) = &r.target else {
        return false;
    };
    let null = matches!(&target[..], [WordPart::Literal(t)] if t == b"/dev/null");
    r.fd == Some(2) && r.kind == RedirKind::Out && null
}

/// Whether `command`'s arguments only describe commands: `-v` or `-V`,
/// possibly with `-p`.
fn describes(argv: &[Vec<u8>]) -> bool {
    argv.get(1).is_some_and(|a| {
        a.len() > 1 && a[0] == b'-' && a[1..].iter().all(|c| b"vVp".contains(c)) && a[1..].iter().any(|&c| c != b'p')
    })
}

/// Whether expanding `parts` has no side effects, and can't fail unless
/// `set -u` or a `glob.fail` option is on. `$RANDOM` changes the generator,
/// and `$LINENO` is the line of the substitution's command in a subshell.
/// With `substs`, a `$(...)` is allowed too, and noted there.
fn pure(parts: &[WordPart], substs: &mut Option<&mut bool>) -> bool {
    parts.iter().all(|p| match p {
        WordPart::Literal(_) | WordPart::SingleQuoted(_) | WordPart::Escaped(_) | WordPart::Tilde(_) => true,
        WordPart::DoubleQuoted(inner) => pure(inner, substs),
        WordPart::CmdSubst(_) => match substs {
            Some(seen) => {
                **seen = true;
                true
            }
            None => false,
        },
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
                | ParamOp::RemoveLargestPrefix(w) => pure(&w.0, substs),
                _ => false,
            };
            name && op && pe.index.is_none() && pe.flags.is_none()
        }
        _ => false,
    })
}

/// What [`Shell::subst_in_place`] made of a `$(...)`.
pub enum Subst {
    /// It ran in place: its output.
    Output(Vec<u8>),
    /// It needs a subshell.
    Subshell,
    /// It needs a subshell, which is to run this list instead: the command
    /// with its words expanded, as they were already.
    Expanded(List),
}

impl Shell {
    /// Whether `PATH` finds the system's `name`, a utility that can run in
    /// the shell (see the module's documentation). `remember` records the
    /// command in the cache (`hash`), as running it would; a subshell's
    /// lookup doesn't.
    fn system_utility(&mut self, name: &[u8], remember: bool) -> bool {
        let path = match self.hash.get(name) {
            Some((p, _)) => p.clone(),
            None if remember => match self.find_in_path(name) {
                Some((p, _)) => p,
                None => return false,
            },
            None => match crate::path::search(&self.get_var(b"PATH").unwrap_or_default(), name) {
                Some((p, _, true)) => p,
                _ => return false,
            },
        };
        let dir = path.strip_suffix(name);
        dir.is_some_and(|d| d == b"/usr/bin/" || d == b"/bin/")
    }

    /// What `argv` prints and its status, if it is a utility run in the
    /// shell (see [`Shell::system_utility`]).
    fn utility_output(&mut self, argv: &[Vec<u8>], remember: bool) -> Option<(Vec<u8>, i32)> {
        let out = output(argv)?;
        self.system_utility(&argv[0], remember).then_some(out)
    }

    /// Runs `argv`, an external command, in the shell if it is a utility
    /// that can run there (see the module's documentation), and returns its
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
        let (out, status) = self.utility_output(argv, true)?;
        if sys::write_all(1, &out) {
            return Some(status);
        }
        self.berr(&argv[0], "write error");
        Some(1)
    }

    /// The value of `$(list)` if it can run without a subshell (see
    /// [`in_place_call`]).
    #[inline(never)]
    pub fn subst_in_place(&mut self, list: &List) -> Result<Subst, Flow> {
        let Some((sc, name, nested)) = in_place_call(list) else {
            return Ok(Subst::Subshell);
        };
        let opts = [Opt::Xtrace, Opt::Nounset, Opt::GlobFail, Opt::GlobFailAll];
        // A subshell's exit runs a CHLD trap.
        let chld = self.traps[libc::SIGCHLD as usize]
            .as_ref()
            .is_some_and(|a| !a.is_empty());
        if chld || opts.into_iter().any(|o| self.opt(o)) {
            return Ok(Subst::Subshell);
        }
        let kind = self.lookup_command(name, true);
        let utility = match kind {
            CommandKind::External if is_utility(name) => true,
            CommandKind::Builtin(_) if !is_utility(name) => false,
            _ => return Ok(Subst::Subshell),
        };
        // Words with a `$(...)` are expanded once, so whether the utility
        // is the system's is decided first.
        if nested && !self.system_utility(name, false) {
            return Ok(Subst::Subshell);
        }
        let argv = self.expand_command_words(&sc.words)?;
        let (mut out, status) = match kind {
            CommandKind::Builtin(f) if !utility => {
                if argv[0] == b"command" && !describes(&argv) {
                    return Ok(Subst::Subshell);
                }
                let Ok(saved) = self.redirect(&sc.redirs, true) else {
                    return Ok(Subst::Subshell);
                };
                let r = self.captured_builtin(f, &argv, sc.lineno);
                self.restore_redirs(saved);
                r
            }
            _ => match output(&argv) {
                Some(r) if nested || self.system_utility(name, false) => r,
                _ if nested => return Ok(Subst::Expanded(expanded(sc, argv))),
                _ => return Ok(Subst::Subshell),
            },
        };
        out.retain(|&b| b != 0);
        while out.last() == Some(&b'\n') {
            out.pop();
        }
        self.subst_status = Some(status);
        Ok(Subst::Output(out))
    }

    /// Runs a built-in that only prints, as a subshell would: returns its
    /// output and status, and forgets the commands it added to the cache.
    fn captured_builtin(&mut self, f: BuiltinFn, argv: &[Vec<u8>], lineno: u32) -> (Vec<u8>, i32) {
        let fresh: Vec<Vec<u8>> = (argv[1..].iter())
            .filter(|a| !self.hash.contains_key(*a))
            .cloned()
            .collect();
        let outer = self.captured.replace(Some(Vec::new()));
        let line = std::mem::replace(&mut self.lineno, lineno);
        let r = self.call_builtin(f, argv);
        self.lineno = line;
        let out = self.captured.replace(outer).unwrap_or_default();
        for name in &fresh {
            self.hash.remove(name);
        }
        let status = match r {
            Ok(n) => n,
            Err(Flow::Exit(n) | Flow::Error(n) | Flow::Return(n)) => n,
            Err(Flow::Break(_) | Flow::Continue(_)) => self.last_status,
        };
        (out, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Option<String> {
        let argv: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes().to_vec()).collect();
        output(&argv).map(|(o, _)| String::from_utf8(o).unwrap().trim_end_matches('\n').to_string())
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

    #[test]
    fn expr() {
        let e = |args: &str| {
            let argv: Vec<Vec<u8>> = std::iter::once("expr")
                .chain(args.split(' '))
                .map(|a| a.into())
                .collect();
            output(&argv).map(|(o, status)| (String::from_utf8(o).unwrap().trim_end_matches('\n').to_string(), status))
        };
        for (args, want, status) in [
            ("1 + 2 * 3", "7", 0),
            ("( 1 + 2 ) * 3", "9", 0),
            ("7 - 7", "0", 1),
            ("7 / -2", "-3", 0),
            ("0 + -7 % 2", "-1", 0),
            ("1 + -1", "0", 1),
            ("007 + 0", "7", 0),
            ("2 < 10", "1", 0),
            ("1 < 2 < 3", "1", 0),
            ("10 = 010", "1", 0),
            ("1 + 1 != 2", "0", 1),
        ] {
            assert_eq!(e(args), Some((want.to_string(), status)), "{args}");
        }
        for args in [
            "007",
            "( 1 )",
            "-1 + 1",
            "-- 1 + 1",
            "+1 + 1",
            "a = a",
            "1 : 1",
            "1 | 2",
            "1 & 2",
            "1 / 0",
            "1 % 0",
            "9223372036854775807 + 1",
            "( 1 + 2",
            "1 + 2 )",
            "1 +",
            "1 + 2 3",
        ] {
            assert_eq!(e(args), None, "{args}");
        }
    }

    #[test]
    fn seq() {
        let seq = |args: &str| {
            let argv: Vec<Vec<u8>> = std::iter::once("seq")
                .chain(args.split(' '))
                .map(|a| a.into())
                .collect();
            output(&argv).map(|(o, _)| String::from_utf8(o).unwrap().replace('\n', " "))
        };
        for (args, want) in [
            ("3", "1 2 3 "),
            ("0", ""),
            ("-2", ""),
            ("2 4", "2 3 4 "),
            ("4 2", ""),
            ("3 3", "3 "),
            ("01 03", "1 2 3 "),
            ("5 -2 1", "5 3 1 "),
            ("1 2 6", "1 3 5 "),
            ("1 -1 3", ""),
            ("-3 -1", "-3 -2 -1 "),
            ("1 100000", &(1..=100_000).map(|i| format!("{i} ")).collect::<String>()),
        ] {
            assert_eq!(seq(args).as_deref(), Some(want), "{args}");
        }
        for args in [
            "",
            "1 2 3 4",
            "1 0 3",
            "-0 2",
            "+1 3",
            "-- 1 3",
            "1.5",
            "1e3",
            "1 100001",
            "1000000000000000",
        ] {
            assert_eq!(seq(args), None, "{args}");
        }
    }

    #[test]
    fn id_uname() {
        let run = |args: &str| {
            let argv: Vec<Vec<u8>> = args.split(' ').map(|a| a.into()).collect();
            output(&argv).map(|(o, _)| String::from_utf8(o).unwrap())
        };
        let uid = format!("{}\n", sys::geteuid());
        assert_eq!(run("id -u").as_ref(), Some(&uid));
        assert_eq!(run("id -r -u").as_ref(), Some(&uid));
        assert_eq!(run("id -gr"), Some(format!("{}\n", sys::getgid())));
        for args in [
            "id",
            "id -G",
            "id -n",
            "id -r",
            "id -ug",
            "id -u root",
            "id -- -u",
            "id -uz",
            "id --user",
        ] {
            assert_eq!(run(args), None, "{args}");
        }
        let [sysname, _, _, _, machine] = sys::uname().unwrap();
        let s = String::from_utf8(sysname).unwrap();
        let m = String::from_utf8(machine).unwrap();
        assert_eq!(run("uname"), Some(format!("{s}\n")));
        assert_eq!(run("uname -m -s"), Some(format!("{s} {m}\n")));
        assert_eq!(run("uname -ms"), Some(format!("{s} {m}\n")));
        for args in [
            "uname -a",
            "uname -o",
            "uname -p",
            "uname -i",
            "uname -s -s",
            "uname --",
            "uname x",
            "uname -",
        ] {
            assert_eq!(run(args), None, "{args}");
        }
    }
}
