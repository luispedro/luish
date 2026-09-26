//! `test` and `[`.

use crate::shell::{ExecResult, Shell};
use crate::sys;

struct Test<'a> {
    sh: &'a Shell,
    args: &'a [Vec<u8>],
    pos: usize,
}

type TResult = Result<bool, String>;

fn is_unary(op: &[u8]) -> bool {
    matches!(
        op,
        b"-b"
            | b"-c"
            | b"-d"
            | b"-e"
            | b"-f"
            | b"-g"
            | b"-h"
            | b"-k"
            | b"-L"
            | b"-n"
            | b"-p"
            | b"-r"
            | b"-s"
            | b"-S"
            | b"-t"
            | b"-u"
            | b"-w"
            | b"-x"
            | b"-z"
            | b"-O"
            | b"-G"
    )
}

fn is_binary(op: &[u8]) -> bool {
    matches!(
        op,
        b"=" | b"!=" | b"<" | b">" | b"-eq" | b"-ne" | b"-gt" | b"-ge" | b"-lt" | b"-le" | b"-nt" | b"-ot" | b"-ef"
    )
}

fn file_type(path: &[u8], kind: u32) -> bool {
    sys::stat(path).is_some_and(|st| st.st_mode & libc::S_IFMT == kind)
}

fn unary(op: &[u8], a: &[u8]) -> TResult {
    let st = || sys::stat(a);
    Ok(match op {
        b"-n" => !a.is_empty(),
        b"-z" => a.is_empty(),
        b"-b" => file_type(a, libc::S_IFBLK),
        b"-c" => file_type(a, libc::S_IFCHR),
        b"-d" => file_type(a, libc::S_IFDIR),
        b"-f" => file_type(a, libc::S_IFREG),
        b"-p" => file_type(a, libc::S_IFIFO),
        b"-S" => file_type(a, libc::S_IFSOCK),
        b"-h" | b"-L" => sys::lstat(a).is_some_and(|st| st.st_mode & libc::S_IFMT == libc::S_IFLNK),
        b"-e" => st().is_some(),
        b"-g" => st().is_some_and(|st| st.st_mode & libc::S_ISGID != 0),
        b"-u" => st().is_some_and(|st| st.st_mode & libc::S_ISUID != 0),
        b"-k" => st().is_some_and(|st| st.st_mode & libc::S_ISVTX != 0),
        b"-s" => st().is_some_and(|st| st.st_size > 0),
        b"-r" => sys::access(a, libc::R_OK),
        b"-w" => sys::access(a, libc::W_OK),
        b"-x" => sys::access(a, libc::X_OK),
        // SAFETY: plain geteuid/getegid.
        b"-O" => st().is_some_and(|st| st.st_uid == unsafe { libc::geteuid() }),
        b"-G" => st().is_some_and(|st| st.st_gid == unsafe { libc::getegid() }),
        b"-t" => {
            let n = parse_int(a)?;
            sys::isatty(n as i32)
        }
        _ => return Err(format!("{}: unexpected operator", String::from_utf8_lossy(op))),
    })
}

fn parse_int(s: &[u8]) -> Result<i64, String> {
    let t = s.trim_ascii();
    let ok = !t.is_empty() && {
        let d = t.strip_prefix(b"-").or_else(|| t.strip_prefix(b"+")).unwrap_or(t);
        !d.is_empty() && d.iter().all(|c| c.is_ascii_digit())
    };
    if !ok {
        return Err(format!("Illegal number: {}", String::from_utf8_lossy(s)));
    }
    std::str::from_utf8(t)
        .unwrap()
        .trim_start_matches('+')
        .parse()
        .map_err(|_| format!("Illegal number: {}", String::from_utf8_lossy(s)))
}

fn mtime(st: &libc::stat) -> (i64, i64) {
    (st.st_mtime, st.st_mtime_nsec)
}

fn binary(a: &[u8], op: &[u8], b: &[u8]) -> TResult {
    Ok(match op {
        b"=" => a == b,
        b"!=" => a != b,
        b"<" => a < b,
        b">" => a > b,
        b"-eq" => parse_int(a)? == parse_int(b)?,
        b"-ne" => parse_int(a)? != parse_int(b)?,
        b"-gt" => parse_int(a)? > parse_int(b)?,
        b"-ge" => parse_int(a)? >= parse_int(b)?,
        b"-lt" => parse_int(a)? < parse_int(b)?,
        b"-le" => parse_int(a)? <= parse_int(b)?,
        b"-nt" => match (sys::stat(a), sys::stat(b)) {
            (Some(x), Some(y)) => mtime(&x) > mtime(&y),
            (Some(_), None) => true,
            _ => false,
        },
        b"-ot" => match (sys::stat(a), sys::stat(b)) {
            (Some(x), Some(y)) => mtime(&x) < mtime(&y),
            (None, Some(_)) => true,
            _ => false,
        },
        b"-ef" => sys::same_file(a, b),
        _ => return Err(format!("{}: unexpected operator", String::from_utf8_lossy(op))),
    })
}

impl Test<'_> {
    fn peek(&self, off: usize) -> Option<&[u8]> {
        self.args.get(self.pos + off).map(|a| a.as_slice())
    }

    fn remaining(&self) -> usize {
        self.args.len() - self.pos
    }

    fn take(&mut self) -> Result<&[u8], String> {
        let a = self.args.get(self.pos).ok_or_else(|| "argument expected".to_string())?;
        self.pos += 1;
        Ok(a)
    }

    fn or_expr(&mut self) -> TResult {
        let mut v = self.and_expr()?;
        while self.peek(0) == Some(b"-o") {
            self.pos += 1;
            let r = self.and_expr()?;
            v = v || r;
        }
        Ok(v)
    }

    fn and_expr(&mut self) -> TResult {
        let mut v = self.not_expr()?;
        while self.peek(0) == Some(b"-a") {
            self.pos += 1;
            let r = self.not_expr()?;
            v = v && r;
        }
        Ok(v)
    }

    fn not_expr(&mut self) -> TResult {
        if self.peek(0) == Some(b"!") && self.remaining() > 1 {
            self.pos += 1;
            return Ok(!self.not_expr()?);
        }
        self.primary()
    }

    fn primary(&mut self) -> TResult {
        let Some(a) = self.peek(0) else {
            return Err("argument expected".into());
        };
        if a == b"(" && self.remaining() > 1 {
            // `( arg )` might be a parenthesized expression or `(` compared
            // with something; follow the usual parenthesized reading.
            self.pos += 1;
            let v = self.or_expr()?;
            if self.peek(0) != Some(b")") {
                return Err("closing paren expected".into());
            }
            self.pos += 1;
            return Ok(v);
        }
        if self.remaining() >= 3 && is_binary(self.peek(1).unwrap()) {
            let a = self.take()?.to_vec();
            let op = self.take()?.to_vec();
            let b = self.take()?.to_vec();
            return binary(&a, &op, &b);
        }
        if is_unary(a) && self.remaining() >= 2 {
            let op = self.take()?.to_vec();
            let b = self.take()?.to_vec();
            return unary(&op, &b);
        }
        let a = self.take()?;
        Ok(!a.is_empty())
    }

    /// Evaluates using the POSIX rules for up to four arguments.
    fn eval(&mut self, n: usize) -> TResult {
        let a = |i: usize| self.args[self.pos + i].as_slice();
        match n {
            0 => Ok(false),
            1 => Ok(!a(0).is_empty()),
            2 => {
                if a(0) == b"!" {
                    return Ok(a(1).is_empty());
                }
                if is_unary(a(0)) {
                    return unary(a(0), a(1));
                }
                Err(format!("{}: unexpected operator", String::from_utf8_lossy(a(0))))
            }
            3 => {
                if is_binary(a(1)) {
                    return binary(a(0), a(1), a(2));
                }
                if a(0) == b"!" {
                    self.pos += 1;
                    return Ok(!self.eval(2)?);
                }
                if a(0) == b"(" && a(2) == b")" {
                    return Ok(!a(1).is_empty());
                }
                self.full()
            }
            4 => {
                if a(0) == b"!" {
                    self.pos += 1;
                    return Ok(!self.eval(3)?);
                }
                if a(0) == b"(" && a(3) == b")" {
                    self.pos += 1;
                    let args = &self.args[..self.pos + 2];
                    let mut sub = Test {
                        sh: self.sh,
                        args,
                        pos: self.pos,
                    };
                    return sub.eval(2);
                }
                self.full()
            }
            _ => self.full(),
        }
    }

    fn full(&mut self) -> TResult {
        let v = self.or_expr()?;
        if let Some(extra) = self.peek(0) {
            return Err(format!("{}: unexpected operator", String::from_utf8_lossy(extra)));
        }
        Ok(v)
    }
}

fn run(sh: &Shell, name: &[u8], args: &[Vec<u8>]) -> i32 {
    let mut t = Test { sh, args, pos: 0 };
    match t.eval(args.len()) {
        Ok(v) => (!v) as i32,
        Err(msg) => {
            sh.berr(name, msg);
            2
        }
    }
}

pub fn test(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    Ok(run(sh, &argv[0], &argv[1..]))
}

pub fn bracket(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.last().map(|a| a.as_slice()) != Some(b"]") {
        sh.berr(&argv[0], "missing ]");
        return Ok(2);
    }
    Ok(run(sh, &argv[0], &argv[1..argv.len() - 1]))
}
