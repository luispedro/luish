//! `test` and `[`.

use crate::shell::{ExecResult, Shell};
use crate::sys;

struct Test<'a> {
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
        // As in dash, both files must exist.
        b"-nt" => match (sys::stat(a), sys::stat(b)) {
            (Some(x), Some(y)) => mtime(&x) > mtime(&y),
            _ => false,
        },
        b"-ot" => match (sys::stat(a), sys::stat(b)) {
            (Some(x), Some(y)) => mtime(&x) < mtime(&y),
            _ => false,
        },
        b"-ef" => sys::same_file(a, b),
        _ => return Err(format!("{}: unexpected operator", String::from_utf8_lossy(op))),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok {
    Eoi,
    Operand,
    Not,
    And,
    Or,
    LParen,
    RParen,
    Unary,
    Binary,
}

/// dash's `syntax`: the message, after the operator if there is one.
fn syntax(op: &[u8], msg: &str) -> String {
    if op.is_empty() {
        msg.to_string()
    } else {
        format!("{}: {msg}", String::from_utf8_lossy(op))
    }
}

fn op_kind(s: &[u8]) -> Option<Tok> {
    Some(match s {
        b"!" => Tok::Not,
        b"-a" => Tok::And,
        b"-o" => Tok::Or,
        b"(" => Tok::LParen,
        b")" => Tok::RParen,
        _ if is_unary(s) => Tok::Unary,
        _ if is_binary(s) => Tok::Binary,
        _ => return None,
    })
}

/// A port of dash's `test` parser (`testcmd`, `t_lex`, `oexpr`...), so
/// that ambiguous expressions are read the same way.
impl Test<'_> {
    fn arg(&self, i: usize) -> Option<&[u8]> {
        self.args.get(i).map(|a| a.as_slice())
    }

    /// dash's `t_lex`: the token at `i`, taking an operator as an operand
    /// where it can't be one.
    fn lex(&self, i: usize) -> Tok {
        let Some(s) = self.arg(i) else {
            return Tok::Eoi;
        };
        match op_kind(s) {
            Some(Tok::Unary) if self.is_operand(i) => Tok::Operand,
            Some(Tok::LParen) if i + 1 >= self.args.len() => Tok::Operand,
            Some(t) => t,
            None => Tok::Operand,
        }
    }

    fn is_operand(&self, i: usize) -> bool {
        if i + 1 >= self.args.len() {
            return true;
        }
        if i + 2 >= self.args.len() {
            return false;
        }
        op_kind(&self.args[i + 1]) == Some(Tok::Binary)
    }

    fn or_expr(&mut self, mut n: Tok) -> TResult {
        let mut res = false;
        loop {
            res |= self.and_expr(n)?;
            if self.lex(self.pos + 1) != Tok::Or {
                return Ok(res);
            }
            self.pos += 2;
            n = self.lex(self.pos);
        }
    }

    fn and_expr(&mut self, mut n: Tok) -> TResult {
        let mut res = true;
        loop {
            if !self.not_expr(n)? {
                res = false;
            }
            if self.lex(self.pos + 1) != Tok::And {
                return Ok(res);
            }
            self.pos += 2;
            n = self.lex(self.pos);
        }
    }

    fn not_expr(&mut self, n: Tok) -> TResult {
        if n != Tok::Not {
            return self.primary(n);
        }
        let n = self.lex(self.pos + 1);
        if n != Tok::Eoi {
            self.pos += 1;
        }
        Ok(!self.not_expr(n)?)
    }

    fn primary(&mut self, n: Tok) -> TResult {
        match n {
            Tok::Eoi => return Ok(false),
            Tok::LParen => {
                self.pos += 1;
                let nn = self.lex(self.pos);
                if nn == Tok::RParen {
                    return Ok(false);
                }
                let res = self.or_expr(nn)?;
                self.pos += 1;
                if self.lex(self.pos) != Tok::RParen {
                    return Err("closing paren expected".into());
                }
                return Ok(res);
            }
            Tok::Unary => {
                let op = self.args[self.pos].clone();
                self.pos += 1;
                let Some(a) = self.arg(self.pos) else {
                    return Err(syntax(&op, "argument expected"));
                };
                return unary(&op, a);
            }
            _ => {}
        }
        if self.lex(self.pos + 1) == Tok::Binary {
            let a = &self.args[self.pos];
            let op = &self.args[self.pos + 1];
            self.pos += 2;
            let Some(b) = self.arg(self.pos) else {
                return Err(syntax(op, "argument expected"));
            };
            return binary(a, op, b);
        }
        Ok(!self.args[self.pos].is_empty())
    }

    /// dash's `testcmd`: the POSIX rules for three and four arguments,
    /// then the parser. Returns the exit status.
    fn run(&mut self) -> Result<i32, String> {
        let mut res = 1;
        loop {
            let argc = self.args.len() - self.pos;
            if argc < 1 {
                return Ok(res);
            }
            if argc == 3 && op_kind(&self.args[self.pos + 1]) == Some(Tok::Binary) {
                return self.finish(res, Tok::Operand);
            }
            if argc == 3 || argc == 4 {
                if self.args[self.pos] == b"(" && self.args[self.args.len() - 1] == b")" {
                    self.args = &self.args[..self.args.len() - 1];
                    self.pos += 1;
                } else if self.args[self.pos] == b"!" {
                    res = 0;
                    self.pos += 1;
                    continue;
                }
            }
            let n = self.lex(self.pos);
            return self.finish(res, n);
        }
    }

    fn finish(&mut self, res: i32, n: Tok) -> Result<i32, String> {
        let v = self.or_expr(n)?;
        if self.pos + 1 < self.args.len() {
            return Err(syntax(&self.args[self.pos], "unexpected operator"));
        }
        Ok(res ^ v as i32)
    }
}

fn run(sh: &Shell, name: &[u8], args: &[Vec<u8>]) -> i32 {
    let mut t = Test { args, pos: 0 };
    match t.run() {
        Ok(status) => status,
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
