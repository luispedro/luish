//! `[[ ... ]]`, as in zsh and bash: the words are expanded without field
//! splitting or globbing, and only as far as the expression is evaluated.

use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;

use crate::ast::{CondExpr, CondOp};
use crate::builtins::test;
use crate::expand::{arith, pattern::Pattern, split::bytes};
use crate::options::{Opt, Options, Setting};
use crate::shell::{ExecResult, Flow, Shell};
use crate::sys;
use crate::vars::Value;

use super::simple::shell_quote;

type CResult = Result<bool, Flow>;

/// The `set -x` trace of the parts evaluated so far, if tracing.
type Trace = Option<Vec<u8>>;

fn push(t: &mut Trace, s: &[u8]) {
    if let Some(t) = t {
        t.extend_from_slice(s);
    }
}

fn push_quoted(t: &mut Trace, s: &[u8]) {
    if let Some(t) = t {
        t.extend(shell_quote(s));
    }
}

impl Shell {
    pub(super) fn run_cond(&mut self, expr: &CondExpr, lineno: u32) -> ExecResult {
        self.lineno = lineno;
        let mut trace = (self.opt(Opt::Xtrace) && !self.in_ps4).then(|| b"[[ ".to_vec());
        let r = self.cond(expr, &mut trace);
        if let Some(mut t) = trace {
            // As in zsh, one line with the parts that were evaluated.
            t.extend_from_slice(b" ]]\n");
            self.in_ps4 = true;
            let mut line = self.expand_prompt(b"PS4");
            self.in_ps4 = false;
            line.extend(t);
            sys::write_all(2, &line);
        }
        Ok(!r? as i32)
    }

    /// Evaluates `e`, in parentheses if it binds less tightly than its
    /// operator (`prec`), for the trace.
    fn cond_operand(&mut self, e: &CondExpr, prec: u8, t: &mut Trace) -> CResult {
        if e.prec() >= prec {
            return self.cond(e, t);
        }
        push(t, b"( ");
        let r = self.cond(e, t);
        push(t, b" )");
        r
    }

    fn cond(&mut self, e: &CondExpr, t: &mut Trace) -> CResult {
        match e {
            CondExpr::Not(a) => {
                push(t, b"! ");
                Ok(!self.cond_operand(a, e.prec(), t)?)
            }
            CondExpr::And(a, b) | CondExpr::Or(a, b) => {
                let and = matches!(e, CondExpr::And(..));
                if self.cond_operand(a, e.prec(), t)? != and {
                    return Ok(!and);
                }
                push(t, if and { b" && " } else { b" || " });
                self.cond_operand(b, e.prec(), t)
            }
            CondExpr::Unary(op, w) => {
                let a = self.expand_word_str(w)?;
                push(t, &[b'-', *op, b' ']);
                push_quoted(t, &a);
                Ok(self.cond_unary(*op, &a))
            }
            CondExpr::Binary(op, l, r) => {
                let a = self.expand_word_str(l)?;
                push_quoted(t, &a);
                push(t, b" ");
                push(t, op.text().as_bytes());
                push(t, b" ");
                if matches!(op, CondOp::Match | CondOp::NoMatch) {
                    // Quoted parts of the pattern match literally, as in `case`.
                    let p = self.expand_pattern(r)?;
                    push_quoted(t, &bytes(&p));
                    return Ok(Pattern::new(&p).matches(&a) == (*op == CondOp::Match));
                }
                let b = self.expand_word_str(r)?;
                push_quoted(t, &b);
                self.cond_binary(*op, &a, &b)
            }
        }
    }

    fn cond_unary(&self, op: u8, a: &[u8]) -> bool {
        match op {
            b'a' => sys::stat(a).is_some(),
            b'o' => match Options::find(a) {
                Some(Setting::Flag(o, on)) => self.opt(o) == on,
                _ => false,
            },
            b'v' => match crate::builtins::parse_uint(a) {
                Some(n) if a.iter().all(u8::is_ascii_digit) => n as usize <= self.positional.len(),
                _ => self.get_var(a).is_some(),
            },
            b'N' => sys::stat(a).is_some_and(|st| (st.st_atime, st.st_atime_nsec) <= (st.st_mtime, st.st_mtime_nsec)),
            b't' => crate::builtins::parse_uint(a).is_some_and(|n| sys::isatty(n as i32)),
            _ => test::unary(&[b'-', op], a).unwrap_or(false),
        }
    }

    fn cond_binary(&mut self, op: CondOp, a: &[u8], b: &[u8]) -> CResult {
        let cmp = match op {
            CondOp::Regex => return self.cond_regex(a, b),
            CondOp::Less => return Ok(a < b),
            CondOp::Greater => return Ok(a > b),
            CondOp::Nt | CondOp::Ot | CondOp::Ef => {
                return Ok(test::binary(a, op.text().as_bytes(), b).unwrap_or(false));
            }
            CondOp::Match | CondOp::NoMatch => unreachable!(),
            _ => (self.cond_arith(a)?).cmp(&self.cond_arith(b)?),
        };
        Ok(match op {
            CondOp::Eq => cmp.is_eq(),
            CondOp::Ne => cmp.is_ne(),
            CondOp::Lt => cmp.is_lt(),
            CondOp::Le => cmp.is_le(),
            CondOp::Gt => cmp.is_gt(),
            _ => cmp.is_ge(),
        })
    }

    /// An operand of `-eq` and the like, an arithmetic expression as in zsh
    /// and bash. An error in it is a shell error, as in `$((...))`.
    fn cond_arith(&mut self, s: &[u8]) -> Result<i64, Flow> {
        arith::eval(self, s).map_err(|msg| {
            self.error(msg);
            Flow::Error(2)
        })
    }

    /// `=~`, with the C library's extended regular expressions. As in zsh,
    /// a match sets `MATCH` to the matched text, and `MBEGIN` and `MEND`
    /// to the offsets of its first and last bytes, from 0 (zsh's sh
    /// emulation, with `KSH_ARRAYS`; from 1 in native zsh). If the regular
    /// expression has groups, the arrays `match`, `mbegin` and `mend` get
    /// the same for each group (empty and -1 for one that took no part).
    /// `BASH_REMATCH` gets the matched text and then each group's, as in
    /// bash (and zsh's `BASH_REMATCH` option). A failed match changes
    /// none of them, as in zsh (bash empties `BASH_REMATCH`).
    fn cond_regex(&mut self, s: &[u8], re: &[u8]) -> CResult {
        // Neither can contain a NUL, as both come from expansions.
        let (Ok(cs), Ok(cre)) = (CString::new(s), CString::new(re)) else {
            return Ok(false);
        };
        let mut preg = MaybeUninit::<libc::regex_t>::uninit();
        // SAFETY: `preg` is written by `regcomp`, and freed only if that
        // succeeded; `cre` is NUL-terminated.
        let rc = unsafe { libc::regcomp(preg.as_mut_ptr(), cre.as_ptr(), libc::REG_EXTENDED) };
        if rc != 0 {
            let mut buf = [0u8; 256];
            // SAFETY: `regerror` writes a NUL-terminated message of at most
            // `buf.len()` bytes.
            unsafe { libc::regerror(rc, preg.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
            let msg = CStr::from_bytes_until_nul(&buf).map(CStr::to_bytes).unwrap_or_default();
            // As in zsh, the condition is false.
            self.error([b"failed to compile regex: ", msg].concat());
            return Ok(false);
        }
        // SAFETY: `preg` was compiled above.
        let groups = unsafe { re_nsub(preg.as_ptr()) };
        let mut m = vec![libc::regmatch_t { rm_so: -1, rm_eo: -1 }; groups + 1];
        // SAFETY: `preg` was compiled above, `cs` is NUL-terminated, and
        // `m` has room for the matches asked for.
        let rc = unsafe {
            let rc = libc::regexec(preg.as_ptr(), cs.as_ptr(), m.len(), m.as_mut_ptr(), 0);
            libc::regfree(preg.as_mut_ptr());
            rc
        };
        if rc != 0 {
            return Ok(false);
        }
        let text = |m: &libc::regmatch_t| match m.rm_so {
            -1 => Vec::new(),
            b => s[b as usize..m.rm_eo as usize].to_vec(),
        };
        let num = |n: i64| n.to_string().into_bytes();
        self.set_var(b"MATCH", text(&m[0]))?;
        self.set_var(b"MBEGIN", num(m[0].rm_so as i64))?;
        self.set_var(b"MEND", num(m[0].rm_eo as i64 - 1))?;
        if groups > 0 {
            let offsets = |end: bool| {
                let off = |g: &libc::regmatch_t| match (g.rm_so, end) {
                    (-1, _) => -1,
                    (b, false) => b as i64,
                    (_, true) => g.rm_eo as i64 - 1,
                };
                Value::Array(Box::new(m[1..].iter().map(|g| num(off(g))).collect()))
            };
            self.set_var_value(b"match", Value::Array(Box::new(m[1..].iter().map(text).collect())))?;
            self.set_var_value(b"mbegin", offsets(false))?;
            self.set_var_value(b"mend", offsets(true))?;
        }
        self.set_var_value(b"BASH_REMATCH", Value::Array(Box::new(m.iter().map(text).collect())))?;
        Ok(true)
    }
}

/// The number of groups in a compiled regular expression: POSIX's
/// `re_nsub`, which the `libc` crate keeps private.
///
/// # Safety
///
/// `preg` must have been compiled by `regcomp`.
unsafe fn re_nsub(preg: *const libc::regex_t) -> usize {
    // glibc's `re_pattern_buffer` has six word-sized fields before it, and
    // musl's `regex_t` starts with it.
    #[cfg(target_env = "gnu")]
    const OFFSET: usize = 6 * size_of::<usize>();
    #[cfg(target_env = "musl")]
    const OFFSET: usize = 0;
    // SAFETY: `re_nsub` is a `size_t` at `OFFSET`, aligned as `regex_t` is.
    unsafe { preg.cast::<u8>().add(OFFSET).cast::<usize>().read() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nsub(re: &str) -> usize {
        let cre = CString::new(re).unwrap();
        let mut preg = MaybeUninit::<libc::regex_t>::uninit();
        // SAFETY: as in `cond_regex`.
        unsafe {
            assert_eq!(libc::regcomp(preg.as_mut_ptr(), cre.as_ptr(), libc::REG_EXTENDED), 0);
            let n = re_nsub(preg.as_ptr());
            libc::regfree(preg.as_mut_ptr());
            n
        }
    }

    #[test]
    fn groups() {
        assert_eq!(nsub("abc"), 0);
        assert_eq!(nsub("a(b)"), 1);
        assert_eq!(nsub("((a)|[(])(b)?\\(c"), 3);
    }
}
