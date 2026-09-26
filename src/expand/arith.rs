//! `$((...))`: arithmetic on signed 64-bit integers, with C operators.

use crate::lexer::{is_name_char, is_name_start};
use crate::shell::Shell;

const OPS: &[&str] = &[
    "<<=", ">>=", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "*=", "/=", "%=", "+=", "-=", "&=", "^=", "|=", "<",
    ">", "=", "+", "-", "*", "/", "%", "&", "^", "|", "!", "~", "?", ":", "(", ")",
];

fn binary_prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | "<=" | ">" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

fn is_assign_op(op: &str) -> bool {
    op.ends_with('=') && !matches!(op, "==" | "!=" | "<=" | ">=")
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
    let digits = std::str::from_utf8(digits).ok()?;
    if digits.starts_with(['+', '-']) {
        return None;
    }
    let v = u64::from_str_radix(digits, radix).ok()? as i64;
    Some(if neg { v.wrapping_neg() } else { v })
}

impl Arith<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek_op(&mut self) -> Option<&'static str> {
        self.skip_ws();
        let rest = &self.s[self.pos..];
        OPS.iter().find(|op| rest.starts_with(op.as_bytes())).copied()
    }

    fn syntax<T>(&self, what: &str) -> Result<T, String> {
        Err(format!(
            "arithmetic expression: {what}: \"{}\"",
            String::from_utf8_lossy(self.s)
        ))
    }

    fn expr(&mut self) -> Result<i64, String> {
        self.skip_ws();
        let save = self.pos;
        if self.pos < self.s.len() && is_name_start(self.s[self.pos]) {
            let name = self.ident();
            if let Some(op) = self.peek_op()
                && is_assign_op(op)
            {
                self.pos += op.len();
                let rhs = self.expr()?;
                let v = if op == "=" {
                    rhs
                } else {
                    let lhs = self.var(&name)?;
                    self.apply(&op[..op.len() - 1], lhs, rhs)?
                };
                if self.noeval == 0 && self.sh.vars.set(&name, v.to_string().into_bytes()).is_err() {
                    return Err(format!("{}: is read only", String::from_utf8_lossy(&name)));
                }
                return Ok(v);
            }
            self.pos = save;
        }
        self.conditional()
    }

    fn conditional(&mut self) -> Result<i64, String> {
        let c = self.binary(1)?;
        if self.peek_op() != Some("?") {
            return Ok(c);
        }
        self.pos += 1;
        if c == 0 {
            self.noeval += 1;
        }
        let a = self.expr()?;
        if c == 0 {
            self.noeval -= 1;
        }
        if self.peek_op() != Some(":") {
            return self.syntax("expecting ':'");
        }
        self.pos += 1;
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
        while let Some(op) = self.peek_op() {
            let Some(prec) = binary_prec(op) else { break };
            if prec < min_prec {
                break;
            }
            self.pos += op.len();
            let skip = match op {
                "&&" => lhs == 0,
                "||" => lhs != 0,
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

    fn apply(&self, op: &str, a: i64, b: i64) -> Result<i64, String> {
        Ok(match op {
            "||" => (a != 0 || b != 0) as i64,
            "&&" => (a != 0 && b != 0) as i64,
            "|" => a | b,
            "^" => a ^ b,
            "&" => a & b,
            "==" => (a == b) as i64,
            "!=" => (a != b) as i64,
            "<" => (a < b) as i64,
            "<=" => (a <= b) as i64,
            ">" => (a > b) as i64,
            ">=" => (a >= b) as i64,
            "<<" => a.wrapping_shl(b as u32),
            ">>" => a.wrapping_shr(b as u32),
            "+" => a.wrapping_add(b),
            "-" => a.wrapping_sub(b),
            "*" => a.wrapping_mul(b),
            "/" | "%" => {
                if b == 0 {
                    if self.noeval > 0 {
                        return Ok(0);
                    }
                    return self.syntax("division by zero");
                }
                if op == "/" {
                    a.wrapping_div(b)
                } else {
                    a.wrapping_rem(b)
                }
            }
            _ => unreachable!("{op}"),
        })
    }

    fn unary(&mut self) -> Result<i64, String> {
        match self.peek_op() {
            Some(op @ ("+" | "-" | "!" | "~")) => {
                self.pos += 1;
                let v = self.unary()?;
                Ok(match op {
                    "+" => v,
                    "-" => v.wrapping_neg(),
                    "!" => (v == 0) as i64,
                    _ => !v,
                })
            }
            _ => self.primary(),
        }
    }

    fn ident(&mut self) -> Vec<u8> {
        let start = self.pos;
        while self.pos < self.s.len() && is_name_char(self.s[self.pos]) {
            self.pos += 1;
        }
        self.s[start..self.pos].to_vec()
    }

    fn var(&mut self, name: &[u8]) -> Result<i64, String> {
        if self.noeval > 0 {
            return Ok(0);
        }
        match self.sh.get_var(name) {
            None => Ok(0),
            Some(v) if v.is_empty() => Ok(0),
            Some(v) => parse_number(&v).ok_or_else(|| format!("Illegal number: {}", String::from_utf8_lossy(&v))),
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
            if self.peek_op() != Some(")") {
                return self.syntax("expecting ')'");
            }
            self.pos += 1;
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
            return self.var(&name);
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
