//! Built-in commands.

mod cd;
mod echo;
mod jobs;
mod misc;
mod printf;
mod read;
mod test;
mod trap;
mod vars;

use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;

pub type BuiltinFn = fn(&mut Shell, &[Vec<u8>]) -> ExecResult;

/// (name, function, special)
const TABLE: &[(&[u8], BuiltinFn, bool)] = &[
    // special built-ins
    (b":", colon, true),
    (b".", misc::dot, true),
    (b"break", break_, true),
    (b"continue", continue_, true),
    (b"eval", eval, true),
    (b"exec", exec, true),
    (b"exit", exit, true),
    (b"export", vars::export, true),
    (b"readonly", vars::readonly, true),
    (b"return", return_, true),
    (b"set", vars::set, true),
    (b"shift", vars::shift, true),
    (b"times", misc::times, true),
    (b"trap", trap::trap, true),
    (b"unset", vars::unset, true),
    // regular built-ins
    (b"[", test::bracket, false),
    (b"alias", misc::alias, false),
    (b"bg", jobs::fg, false),
    (b"cd", cd::cd, false),
    (b"command", misc::command, false),
    (b"echo", echo::echo, false),
    (b"false", false_, false),
    (b"fg", jobs::fg, false),
    (b"getopts", read::getopts, false),
    (b"hash", misc::hash, false),
    (b"jobs", jobs::jobs, false),
    (b"kill", jobs::kill, false),
    (b"local", vars::local, false),
    (b"printf", printf::printf, false),
    (b"pwd", cd::pwd, false),
    (b"read", read::read, false),
    (b"test", test::test, false),
    (b"true", colon, false),
    (b"type", misc::type_, false),
    (b"ulimit", misc::ulimit, false),
    (b"umask", misc::umask, false),
    (b"unalias", misc::unalias, false),
    (b"wait", jobs::wait, false),
];

pub fn lookup(name: &[u8]) -> Option<(BuiltinFn, bool)> {
    TABLE.iter().find(|b| b.0 == name).map(|b| (b.1, b.2))
}

/// The names of all built-ins.
pub fn names() -> impl Iterator<Item = &'static [u8]> {
    TABLE.iter().map(|b| b.0)
}

impl Shell {
    /// Writes built-in output to stdout.
    pub fn out(&self, s: &[u8]) -> bool {
        sys::write_all(1, s)
    }

    /// Prints `$0: LINENO: name: msg`.
    pub fn berr(&self, name: &[u8], msg: impl AsRef<str>) {
        self.error(format!("{}: {}", String::from_utf8_lossy(name), msg.as_ref()));
    }

    /// Reports a write error from a built-in.
    pub fn out_or_err(&self, name: &[u8], s: &[u8]) -> i32 {
        if self.out(s) {
            0
        } else {
            self.berr(name, "I/O error");
            1
        }
    }
}

/// Parses a non-negative decimal number, as for `exit`, `shift`, `break`.
pub fn parse_uint(s: &[u8]) -> Option<i64> {
    if s.is_empty() || !s.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
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
    sh.exec_argv(args)
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
