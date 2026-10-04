//! `$((...))`: arithmetic on signed 64-bit integers, with C operators.

use crate::lexer::{is_name_char, is_name_start};
use crate::shell::Shell;
use crate::vars::{AssignError, Subscript};

/// A binary operator (also the operator of a compound assignment, and `+`
/// and `-` as unary operators).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bin {
    LogOr,
    LogAnd,
    Or,
    Xor,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Bin {
    fn prec(self) -> u8 {
        match self {
            Bin::LogOr => 1,
            Bin::LogAnd => 2,
            Bin::Or => 3,
            Bin::Xor => 4,
            Bin::And => 5,
            Bin::Eq | Bin::Ne => 6,
            Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge => 7,
            Bin::Shl | Bin::Shr => 8,
            Bin::Add | Bin::Sub => 9,
            Bin::Mul | Bin::Div | Bin::Rem => 10,
        }
    }
}

/// An operator token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Bin(Bin),
    /// `=`, or a compound assignment such as `+=`.
    Assign(Option<Bin>),
    Not,
    Compl,
    Quest,
    Colon,
    LParen,
    RParen,
}

/// The operator at the start of `s` and its length (the longest match).
fn lex_op(s: &[u8]) -> Option<(Op, usize)> {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let (bin, len) = match at(0) {
        b'|' if at(1) == b'|' => return Some((Op::Bin(Bin::LogOr), 2)),
        b'&' if at(1) == b'&' => return Some((Op::Bin(Bin::LogAnd), 2)),
        b'=' if at(1) == b'=' => return Some((Op::Bin(Bin::Eq), 2)),
        b'!' if at(1) == b'=' => return Some((Op::Bin(Bin::Ne), 2)),
        b'<' if at(1) == b'<' => (Bin::Shl, 2),
        b'>' if at(1) == b'>' => (Bin::Shr, 2),
        b'<' if at(1) == b'=' => return Some((Op::Bin(Bin::Le), 2)),
        b'>' if at(1) == b'=' => return Some((Op::Bin(Bin::Ge), 2)),
        b'<' => return Some((Op::Bin(Bin::Lt), 1)),
        b'>' => return Some((Op::Bin(Bin::Gt), 1)),
        b'=' => return Some((Op::Assign(None), 1)),
        b'!' => return Some((Op::Not, 1)),
        b'~' => return Some((Op::Compl, 1)),
        b'?' => return Some((Op::Quest, 1)),
        b':' => return Some((Op::Colon, 1)),
        b'(' => return Some((Op::LParen, 1)),
        b')' => return Some((Op::RParen, 1)),
        b'|' => (Bin::Or, 1),
        b'^' => (Bin::Xor, 1),
        b'&' => (Bin::And, 1),
        b'+' => (Bin::Add, 1),
        b'-' => (Bin::Sub, 1),
        b'*' => (Bin::Mul, 1),
        b'/' => (Bin::Div, 1),
        b'%' => (Bin::Rem, 1),
        _ => return None,
    };
    if at(len) == b'=' {
        Some((Op::Assign(Some(bin)), len + 1))
    } else {
        Some((Op::Bin(bin), len))
    }
}

struct Arith<'a> {
    s: &'a [u8],
    pos: usize,
    sh: &'a mut Shell,
    /// Inside the unevaluated side of `&&`, `||` or `?:`.
    noeval: u32,
}

/// Parses an integer constant as the shell does for arithmetic: decimal,
/// octal with a leading `0`, or hex with `0x`.
pub fn parse_number(s: &[u8]) -> Option<i64> {
    let t = s.trim_ascii();
    let (neg, t) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if t.is_empty() {
        return None;
    }
    let (radix, digits) = if t.len() > 2 && (t.starts_with(b"0x") || t.starts_with(b"0X")) {
        (16, &t[2..])
    } else if t.len() > 1 && t[0] == b'0' {
        (8, &t[1..])
    } else {
        (10, t)
    };
    let mut v: u64 = 0;
    for &c in digits {
        let d = (c as char).to_digit(radix)?;
        v = v.checked_mul(radix as u64)?.checked_add(d as u64)?;
    }
    let v = v as i64;
    Some(if neg { v.wrapping_neg() } else { v })
}

impl<'a> Arith<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek_op(&mut self) -> Option<(Op, usize)> {
        self.skip_ws();
        lex_op(&self.s[self.pos..])
    }

    /// Consumes the operator `op` if it comes next.
    fn eat(&mut self, op: Op) -> bool {
        match self.peek_op() {
            Some((o, len)) if o == op => {
                self.pos += len;
                true
            }
            _ => false,
        }
    }

    fn syntax<T>(&self, what: &str) -> Result<T, String> {
        Err(format!(
            "arithmetic expression: {what}: \"{}\"",
            String::from_utf8_lossy(self.s)
        ))
    }

    fn expr(&mut self) -> Result<i64, String> {
        // Parentheses and assignments nest through here, unary operators
        // through unary().
        if !crate::stack::ok() {
            return Err(crate::stack::TOO_DEEP.into());
        }
        self.skip_ws();
        let save = self.pos;
        if self.pos < self.s.len() && is_name_start(self.s[self.pos]) {
            let name = self.ident();
            // An array element, `a[i] = v`: the index is evaluated only if
            // an assignment follows.
            let bracket = self.pos;
            let end = if self.s.get(self.pos) == Some(&b'[') {
                self.pos = self.closing_bracket(bracket)?;
                Some(self.pos)
            } else {
                None
            };
            if let Some((Op::Assign(bin), len)) = self.peek_op() {
                let after = self.pos + len;
                let index = match end {
                    Some(end) => Some(self.subscript(name, bracket, end)?),
                    None => None,
                };
                self.pos = after;
                let rhs = self.expr()?;
                let v = match bin {
                    None => rhs,
                    Some(bin) => {
                        let lhs = match &index {
                            Some(sub) => self.element(name, sub)?,
                            None => self.var(name)?,
                        };
                        self.apply(bin, lhs, rhs)?
                    }
                };
                if self.noeval == 0 {
                    let value = v.to_string().into_bytes();
                    let r = match &index {
                        Some(sub) => self.sh.vars.set_element(name, sub, value, false),
                        None => self.sh.vars.set(name, value).map_err(Into::into),
                    };
                    match r {
                        Ok(()) if self.sh.vartrace.is_some() => self.sh.trace_set(name),
                        Ok(()) => {}
                        Err(AssignError::Readonly) => {
                            return Err(format!("{}: is read only", String::from_utf8_lossy(name)));
                        }
                        Err(AssignError::BadSubscript) => {
                            let sub = index.unwrap_or(Subscript::Index(0));
                            return Err(format!("{}[{sub}]: bad array subscript", String::from_utf8_lossy(name)));
                        }
                    }
                }
                return Ok(v);
            }
            self.pos = save;
        }
        self.conditional()
    }

    /// The position after the `]` that closes the `[` at `open`.
    fn closing_bracket(&self, open: usize) -> Result<usize, String> {
        let mut depth = 0;
        for (i, &c) in self.s.iter().enumerate().skip(open) {
            match c {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i + 1);
                    }
                }
                _ => {}
            }
        }
        self.syntax("expecting ']'")
    }

    /// The subscript of `name` between the `[` at `open` and the `]` before
    /// `end`, leaving the position at `end`: for an associative array, the
    /// text is the key (as in zsh and bash), otherwise it is evaluated.
    fn subscript(&mut self, name: &[u8], open: usize, end: usize) -> Result<Subscript, String> {
        if self.sh.vars.is_assoc(name) {
            self.pos = end;
            return Ok(Subscript::Key(self.s[open + 1..end - 1].to_vec()));
        }
        self.pos = open + 1;
        let i = self.expr()?;
        self.skip_ws();
        if self.pos != end - 1 {
            return self.syntax("expecting ']'");
        }
        self.pos = end;
        Ok(Subscript::Index(i))
    }

    /// The value of an array element, as [`Arith::var`].
    fn element(&mut self, name: &[u8], sub: &Subscript) -> Result<i64, String> {
        if self.noeval > 0 {
            return Ok(0);
        }
        match self.sh.element(name, sub) {
            None => Ok(0),
            Some(v) if v.trim_ascii().is_empty() => Ok(0),
            Some(v) => parse_number(&v).ok_or_else(|| format!("Illegal number: {}", String::from_utf8_lossy(&v))),
        }
    }

    fn conditional(&mut self) -> Result<i64, String> {
        let c = self.binary(1)?;
        if !self.eat(Op::Quest) {
            return Ok(c);
        }
        if c == 0 {
            self.noeval += 1;
        }
        let a = self.expr()?;
        if c == 0 {
            self.noeval -= 1;
        }
        if !self.eat(Op::Colon) {
            return self.syntax("expecting ':'");
        }
        if c != 0 {
            self.noeval += 1;
        }
        let b = self.conditional()?;
        if c != 0 {
            self.noeval -= 1;
        }
        Ok(if c != 0 { a } else { b })
    }

    fn binary(&mut self, min_prec: u8) -> Result<i64, String> {
        let mut lhs = self.unary()?;
        while let Some((Op::Bin(op), len)) = self.peek_op() {
            let prec = op.prec();
            if prec < min_prec {
                break;
            }
            self.pos += len;
            let skip = match op {
                Bin::LogAnd => lhs == 0,
                Bin::LogOr => lhs != 0,
                _ => false,
            };
            if skip {
                self.noeval += 1;
            }
            let rhs = self.binary(prec + 1)?;
            if skip {
                self.noeval -= 1;
            }
            lhs = self.apply(op, lhs, rhs)?;
        }
        Ok(lhs)
    }

    fn apply(&self, op: Bin, a: i64, b: i64) -> Result<i64, String> {
        Ok(match op {
            Bin::LogOr => (a != 0 || b != 0) as i64,
            Bin::LogAnd => (a != 0 && b != 0) as i64,
            Bin::Or => a | b,
            Bin::Xor => a ^ b,
            Bin::And => a & b,
            Bin::Eq => (a == b) as i64,
            Bin::Ne => (a != b) as i64,
            Bin::Lt => (a < b) as i64,
            Bin::Le => (a <= b) as i64,
            Bin::Gt => (a > b) as i64,
            Bin::Ge => (a >= b) as i64,
            Bin::Shl => a.wrapping_shl(b as u32),
            Bin::Shr => a.wrapping_shr(b as u32),
            Bin::Add => a.wrapping_add(b),
            Bin::Sub => a.wrapping_sub(b),
            Bin::Mul => a.wrapping_mul(b),
            Bin::Div | Bin::Rem => {
                if b == 0 {
                    if self.noeval > 0 {
                        return Ok(0);
                    }
                    return self.syntax("division by zero");
                }
                if op == Bin::Div {
                    a.wrapping_div(b)
                } else {
                    a.wrapping_rem(b)
                }
            }
        })
    }

    fn unary(&mut self) -> Result<i64, String> {
        let op = match self.peek_op() {
            Some((op @ (Op::Bin(Bin::Add | Bin::Sub) | Op::Not | Op::Compl), _)) => op,
            _ => return self.primary(),
        };
        self.pos += 1;
        if !crate::stack::ok() {
            return Err(crate::stack::TOO_DEEP.into());
        }
        let v = self.unary()?;
        Ok(match op {
            Op::Bin(Bin::Add) => v,
            Op::Bin(Bin::Sub) => v.wrapping_neg(),
            Op::Not => (v == 0) as i64,
            _ => !v,
        })
    }

    fn ident(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.pos < self.s.len() && is_name_char(self.s[self.pos]) {
            self.pos += 1;
        }
        let s = self.s;
        &s[start..self.pos]
    }

    fn var(&mut self, name: &[u8]) -> Result<i64, String> {
        if self.noeval > 0 {
            return Ok(0);
        }
        if name == b"LINENO" {
            return Ok(self.sh.lineno.into());
        }
        match self.sh.vars.get(name) {
            None => Ok(self.sh.special_value(name).and_then(|v| parse_number(&v)).unwrap_or(0)),
            Some(v) if v.trim_ascii().is_empty() => Ok(0),
            Some(v) => parse_number(v).ok_or_else(|| format!("Illegal number: {}", String::from_utf8_lossy(v))),
        }
    }

    fn primary(&mut self) -> Result<i64, String> {
        self.skip_ws();
        let Some(&c) = self.s.get(self.pos) else {
            return self.syntax("expecting primary");
        };
        if c == b'(' {
            self.pos += 1;
            let v = self.expr()?;
            if !self.eat(Op::RParen) {
                return self.syntax("expecting ')'");
            }
            return Ok(v);
        }
        if c.is_ascii_digit() {
            let start = self.pos;
            while self.pos < self.s.len() && is_name_char(self.s[self.pos]) {
                self.pos += 1;
            }
            return match parse_number(&self.s[start..self.pos]) {
                Some(v) => Ok(v),
                None => self.syntax("expecting primary"),
            };
        }
        if is_name_start(c) {
            let name = self.ident();
            if self.s.get(self.pos) == Some(&b'[') {
                let open = self.pos;
                let end = self.closing_bracket(open)?;
                let sub = self.subscript(name, open, end)?;
                return self.element(name, &sub);
            }
            return self.var(name);
        }
        self.syntax("expecting primary")
    }
}

pub fn eval(sh: &mut Shell, s: &[u8]) -> Result<i64, String> {
    let mut a = Arith {
        s,
        pos: 0,
        sh,
        noeval: 0,
    };
    a.skip_ws();
    if a.pos == s.len() {
        return Ok(0);
    }
    let v = a.expr()?;
    a.skip_ws();
    if a.pos != s.len() {
        return a.syntax("expecting EOF");
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(s: &str) -> Result<i64, String> {
        let mut sh = Shell::new();
        eval(&mut sh, s.as_bytes())
    }

    #[test]
    fn arithmetic() {
        assert_eq!(ev("1 + 2 * 3"), Ok(7));
        assert_eq!(ev("(1 + 2) * 3"), Ok(9));
        assert_eq!(ev("010 + 0x10"), Ok(24));
        assert_eq!(ev("-3 / 2"), Ok(-1));
        assert_eq!(ev("7 % 3"), Ok(1));
        assert_eq!(ev("1 << 4 | 1"), Ok(17));
        assert_eq!(ev("!0 && 2 > 1"), Ok(1));
        assert_eq!(ev("0 && 1/0"), Ok(0));
        assert_eq!(ev("1 ? 2 : 1/0"), Ok(2));
        assert!(ev("x = 5, 1").is_err());
        assert_eq!(ev("x = 5"), Ok(5));
        assert_eq!(ev("~0"), Ok(-1));
        assert!(ev("1/0").is_err());
        assert!(ev("1 +").is_err());
        assert_eq!(ev(""), Ok(0));
    }

    #[test]
    fn variables() {
        let mut sh = Shell::new();
        eval(&mut sh, b"x = 3").unwrap();
        assert_eq!(eval(&mut sh, b"x += 4"), Ok(7));
        assert_eq!(eval(&mut sh, b"x * 2"), Ok(14));
        assert_eq!(eval(&mut sh, b"unset_var + 1"), Ok(1));
    }
}
