//! Built-in commands.

pub mod cd;
mod dirstack;
mod echo;
mod fc;
mod help;
pub mod internal;
mod jobs;
pub(crate) mod misc;
mod print;
mod printf;
mod read;
pub(crate) mod test;
mod trap;
mod vars;

pub use vars::{quote_value, single_quote};

use crate::options::Opt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

pub type BuiltinFn = fn(&mut Shell, &[Vec<u8>]) -> ExecResult;

/// Defines `TABLE`, the built-ins as (name, function, special), and
/// `lookup`, which finds one with a `match` (compiled to comparisons of the
/// length and the bytes as integers, rather than a scan of the table).
macro_rules! builtins {
    ($(($name:literal, $f:expr, $special:expr),)*) => {
        const TABLE: &[(&[u8], BuiltinFn, bool)] = &[$(($name, $f, $special)),*];

        /// Finds a built-in that every shell has: (function, special). See
        /// [`Shell::builtin`] for all of them.
        pub fn lookup(name: &[u8]) -> Option<(BuiltinFn, bool)> {
            match name {
                $($name => Some(($f, $special)),)*
                _ => None,
            }
        }
    };
}

builtins! {
    // special built-ins
    (b":", colon, true),
    (b".", misc::dot, true),
    (b"break", break_, true),
    (b"continue", continue_, true),
    (b"eval", eval, true),
    (b"exec", exec, true),
    (b"exit", exit, true),
    (b"export", vars::export, true),
    // Special in dash (though not in POSIX).
    (b"local", vars::local, true),
    (b"readonly", vars::readonly, true),
    (b"return", return_, true),
    (b"set", vars::set, true),
    (b"shift", vars::shift, true),
    // Not POSIX: special, as in zsh's sh emulation.
    (b"source", misc::source, true),
    (b"times", misc::times, true),
    (b"trap", trap::trap, true),
    (b"unset", vars::unset, true),
    // regular built-ins
    (b"__luish_internal", internal::internal, false),
    (b"[", test::bracket, false),
    (b"alias", misc::alias, false),
    (b"bg", jobs::fg, false),
    // Not POSIX: as in zsh and bash.
    (b"builtin", misc::builtin, false),
    (b"cd", cd::cd, false),
    (b"chdir", cd::cd, false),
    (b"command", misc::command, false),
    // Not POSIX: as in zsh and bash.
    (b"declare", vars::typeset, false),
    // Not POSIX: as in zsh.
    (b"dirs", dirstack::dirs, false),
    (b"echo", echo::echo, false),
    (b"false", false_, false),
    (b"fc", fc::fc, false),
    (b"fg", jobs::fg, false),
    (b"getopts", read::getopts, false),
    (b"hash", misc::hash, false),
    (b"jobs", jobs::jobs, false),
    (b"kill", jobs::kill, false),
    (b"let", misc::let_, false),
    (b"popd", dirstack::popd, false),
    (b"printf", printf::printf, false),
    (b"pushd", dirstack::pushd, false),
    (b"pwd", cd::pwd, false),
    (b"read", read::read, false),
    // Not POSIX: as in zsh.
    (b"setopt", vars::setopt, false),
    (b"test", test::test, false),
    (b"true", colon, false),
    (b"type", misc::type_, false),
    // Not POSIX: as in zsh and bash.
    (b"typeset", vars::typeset, false),
    (b"ulimit", misc::ulimit, false),
    (b"umask", misc::umask, false),
    (b"unalias", misc::unalias, false),
    (b"unsetopt", vars::setopt, false),
    (b"wait", jobs::wait, false),
}

/// Regular built-ins that exist only in shells started interactive (and
/// their subshells), so that scripts find the same commands as in dash.
const INTERACTIVE: &[(&[u8], BuiltinFn)] = &[
    (b"bindkey", crate::interactive::keys::bindkey),
    (b"help", help::help),
    (b"plugin", crate::plugins::plugin),
    (b"print", print::print),
];

/// The names of all built-ins, including the interactive-only ones (for
/// the line editor).
pub fn names() -> impl Iterator<Item = &'static [u8]> {
    TABLE.iter().map(|b| b.0).chain(INTERACTIVE.iter().map(|b| b.0))
}

/// Parses the options of a built-in (dash's `nextopt`): `allowed` are the
/// option letters. Returns the option letters given and the operands, or
/// the exit status after an invalid option.
pub fn options<'a>(sh: &Shell, argv: &'a [Vec<u8>], allowed: &[u8]) -> Result<(Vec<u8>, &'a [Vec<u8>]), i32> {
    let mut opts = Vec::new();
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        for &c in &a[1..] {
            if !allowed.contains(&c) {
                sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                return Err(2);
            }
            opts.push(c);
        }
    }
    Ok((opts, &argv[i..]))
}

impl Shell {
    /// Finds a built-in: (function, special). The `-i` option can't be
    /// changed after startup, so it tells whether the shell was started
    /// interactive, also in subshells.
    pub fn builtin(&self, name: &[u8]) -> Option<(BuiltinFn, bool)> {
        lookup(name).or_else(|| {
            if !self.opt(Opt::Interactive) {
                return None;
            }
            INTERACTIVE.iter().find(|b| b.0 == name).map(|b| (b.1, false))
        })
    }

    /// Writes built-in output to stdout.
    /// Writes built-in output to stdout. A failure is reported after the
    /// built-in returns (see [`Shell::call_builtin`]).
    pub fn out(&self, s: &[u8]) -> bool {
        let ok = sys::write_all(1, s);
        if !ok {
            self.out_failed.set(true);
        }
        ok
    }

    /// Runs a built-in. As in dash's `evalbltin`, if writing its output
    /// failed, `name: I/O error` is printed and the status gets bit 1.
    pub fn call_builtin(&mut self, f: BuiltinFn, argv: &[Vec<u8>]) -> ExecResult {
        let r = f(self, argv);
        if !self.out_failed.replace(false) {
            return r;
        }
        self.berr(&argv[0], "I/O error");
        r.map(|status| status | 1)
    }

    /// Prints `$0: LINENO: name: msg`.
    pub fn berr(&self, name: &[u8], msg: impl AsRef<str>) {
        self.error(format!("{}: {}", String::from_utf8_lossy(name), msg.as_ref()));
    }

    /// Reports a write error from a built-in.
    /// [`Shell::out`] for a built-in whose status is 0 when it gets here.
    pub fn out_status(&self, s: &[u8]) -> i32 {
        self.out(s);
        0
    }
}

/// Parses a non-negative decimal number, as for `exit`, `shift`, `break`.
/// dash's `number`: a decimal number (with optional blanks around it and a
/// sign, as `strtoimax` reads it) from 0 to `INT_MAX`.
pub fn parse_uint(s: &[u8]) -> Option<i64> {
    let t = s.trim_ascii_start();
    let (neg, rest) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let end = rest.iter().position(|c| !c.is_ascii_digit()).unwrap_or(rest.len());
    if end == 0 || !rest[end..].iter().all(|c| c.is_ascii_whitespace()) {
        return None;
    }
    let n: i64 = std::str::from_utf8(&rest[..end]).ok()?.parse().ok()?;
    let n = if neg { -n } else { n };
    (0..=i32::MAX as i64).contains(&n).then_some(n)
}

fn illegal_number(sh: &Shell, name: &[u8], arg: &[u8]) -> Flow {
    sh.berr(name, format!("Illegal number: {}", String::from_utf8_lossy(arg)));
    Flow::Error(2)
}

fn colon(_: &mut Shell, _: &[Vec<u8>]) -> ExecResult {
    Ok(0)
}

fn false_(_: &mut Shell, _: &[Vec<u8>]) -> ExecResult {
    Ok(1)
}

fn loop_count(sh: &Shell, argv: &[Vec<u8>]) -> Result<usize, Flow> {
    match argv.get(1) {
        None => Ok(1),
        Some(a) => match parse_uint(a) {
            Some(n) if n > 0 => Ok(n as usize),
            _ => Err(illegal_number(sh, &argv[0], a)),
        },
    }
}

fn break_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let n = loop_count(sh, argv)?;
    if sh.loop_depth == 0 {
        return Ok(0);
    }
    Err(Flow::Break(n.min(sh.loop_depth)))
}

fn continue_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let n = loop_count(sh, argv)?;
    if sh.loop_depth == 0 {
        return Ok(0);
    }
    Err(Flow::Continue(n.min(sh.loop_depth)))
}

fn eval(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let text = argv[1..].join(&b' ');
    sh.run_string(&text)
}

fn exec(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut args = &argv[1..];
    if args.first().is_some_and(|a| a == b"--") {
        args = &args[1..];
    }
    if args.is_empty() {
        return Ok(0);
    }
    sh.exec_argv(args, None)
}

fn exit_status_arg(sh: &Shell, argv: &[Vec<u8>]) -> Result<i32, Flow> {
    match argv.get(1) {
        None => Ok(sh.last_status),
        Some(a) => match parse_uint(a) {
            Some(n) => Ok(n as i32),
            None => Err(illegal_number(sh, &argv[0], a)),
        },
    }
}

fn exit(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if sh.stopped_jobs_warning() {
        return Ok(0);
    }
    let n = exit_status_arg(sh, argv)?;
    Err(Flow::Exit(n))
}

fn return_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let n = exit_status_arg(sh, argv)?;
    Err(Flow::Return(n))
}
