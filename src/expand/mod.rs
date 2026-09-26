//! Word expansion (XCU §2.6): tilde, parameter, command substitution,
//! arithmetic, field splitting, pathname expansion, and quote removal.

pub mod arith;
pub mod glob;
pub mod pattern;
pub mod split;

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;
use pattern::{Trim, has_meta};
use split::{Fields, XField, bytes};

type EResult<T> = Result<T, Flow>;

impl Shell {
    fn ifs(&self) -> Vec<u8> {
        self.vars
            .get(b"IFS")
            .map(|v| v.to_vec())
            .unwrap_or_else(|| b" \t\n".to_vec())
    }

    /// Full expansion of command words: produces zero or more fields each.
    pub fn expand_words(&mut self, words: &[Word]) -> EResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(words.len());
        for w in words {
            if let Some(lit) = w.as_literal()
                && (self.opt(Opt::Noglob) || !lit.iter().any(|c| matches!(c, b'*' | b'?' | b'[')) || lit == b"[")
            {
                out.push(lit.to_vec());
                continue;
            }
            let mut f = Fields::new(Some(self.ifs()));
            self.expand_parts(&w.0, false, false, &mut f)?;
            for field in f.into_fields() {
                self.glob_field(field, &mut out);
            }
        }
        Ok(out)
    }

    fn glob_field(&self, field: XField, out: &mut Vec<Vec<u8>>) {
        if !self.opt(Opt::Noglob) && has_meta(&field) {
            let matches = glob::glob(&field);
            if !matches.is_empty() {
                out.extend(matches);
                return;
            }
        }
        out.push(bytes(&field));
    }

    /// Expansion without field splitting or globbing (assignments,
    /// redirection targets, `case` words, here-documents).
    pub fn expand_word_str(&mut self, w: &Word) -> EResult<Vec<u8>> {
        if let Some(lit) = w.as_literal() {
            return Ok(lit.to_vec());
        }
        let mut f = Fields::new(None);
        self.expand_parts(&w.0, false, false, &mut f)?;
        let fields = f.into_fields();
        Ok(fields.first().map(|f| bytes(f)).unwrap_or_default())
    }

    /// Expansion into a pattern: no splitting, and quoting is kept so that
    /// quoted metacharacters match literally.
    pub fn expand_pattern(&mut self, w: &Word) -> EResult<XField> {
        let mut f = Fields::new(None);
        self.expand_parts(&w.0, false, false, &mut f)?;
        Ok(f.into_fields().pop().unwrap_or_default())
    }

    /// `quoted`: inside double quotes. `lit_exp`: literal text counts as
    /// the result of an expansion (the word in an unquoted `${x-word}`).
    fn expand_parts(&mut self, parts: &[WordPart], quoted: bool, lit_exp: bool, f: &mut Fields) -> EResult<()> {
        for part in parts {
            self.expand_part(part, quoted, lit_exp, f)?;
        }
        Ok(())
    }

    fn expand_part(&mut self, part: &WordPart, quoted: bool, lit_exp: bool, f: &mut Fields) -> EResult<()> {
        match part {
            WordPart::Literal(s) => {
                if quoted {
                    f.push_quoted(s);
                } else if lit_exp {
                    f.push_expansion(s);
                } else {
                    f.push_literal(s);
                }
            }
            WordPart::SingleQuoted(s) => f.push_quoted(s),
            WordPart::Escaped(c) => f.push_quoted(&[*c]),
            WordPart::DoubleQuoted(inner) => {
                if inner.is_empty() {
                    f.cur_exists = true;
                }
                self.expand_parts(inner, true, false, f)?;
            }
            WordPart::Tilde(user) => {
                let home = if user.is_empty() {
                    self.get_var(b"HOME").or_else(sys::own_home_dir)
                } else {
                    sys::home_dir(user)
                };
                match home {
                    Some(h) => f.push_quoted(&h),
                    None => {
                        f.push_literal(b"~");
                        f.push_literal(user);
                    }
                }
            }
            WordPart::Param(pe) => self.expand_param(pe, quoted, f)?,
            WordPart::CmdSubst(list) => {
                let out = self.command_subst(list)?;
                push_result(&out, quoted, f);
            }
            WordPart::Arith(w) => {
                let s = self.expand_word_str(w)?;
                match arith::eval(self, &s) {
                    Ok(v) => push_result(v.to_string().as_bytes(), quoted, f),
                    Err(msg) => {
                        self.error(msg);
                        return Err(Flow::Error(2));
                    }
                }
            }
        }
        Ok(())
    }

    fn param_value(&self, name: &ParamName) -> Option<Vec<u8>> {
        match name {
            ParamName::Var(n) => self.get_var(n),
            ParamName::Positional(n) => self.positional.get(n - 1).cloned(),
            ParamName::Special(c) => Some(match c {
                b'?' => self.last_status.to_string().into_bytes(),
                b'$' => self.pid.to_string().into_bytes(),
                b'!' => return self.last_bg_pid.map(|p| p.to_string().into_bytes()),
                b'#' => self.positional.len().to_string().into_bytes(),
                b'-' => self.options.letters(),
                b'0' => self.arg0.clone(),
                b'@' | b'*' => {
                    if self.positional.is_empty() {
                        return None;
                    }
                    self.positional.join(&b' ')
                }
                _ => return None,
            }),
        }
    }

    fn unset_error(&self, name: &ParamName, msg: &str) -> Flow {
        self.error(format!("{}: {msg}", param_display(name)));
        Flow::Error(2)
    }

    fn expand_param(&mut self, pe: &ParamExp, quoted: bool, f: &mut Fields) -> EResult<()> {
        let multi = matches!(pe.name, ParamName::Special(b'@' | b'*'));
        let val = self.param_value(&pe.name);
        let nounset = self.opt(Opt::Nounset) && !multi;
        if let ParamOp::Length = pe.op {
            let n = if multi {
                self.positional.len()
            } else {
                match &val {
                    Some(v) => v.len(),
                    None if nounset => return Err(self.unset_error(&pe.name, "parameter not set")),
                    None => 0,
                }
            };
            push_result(n.to_string().as_bytes(), quoted, f);
            return Ok(());
        }
        let is_set = match &val {
            Some(v) => !pe.colon || !v.is_empty(),
            None => false,
        };
        let push_value = |sh: &mut Shell, f: &mut Fields| {
            if multi {
                sh.push_positional(pe.name == ParamName::Special(b'@'), quoted, f);
            } else if let Some(v) = &val {
                push_result(v, quoted, f);
            }
        };
        match &pe.op {
            ParamOp::Plain => {
                if val.is_none() && nounset {
                    return Err(self.unset_error(&pe.name, "parameter not set"));
                }
                push_value(self, f);
            }
            ParamOp::Default(w) => {
                if is_set {
                    push_value(self, f);
                } else {
                    self.expand_parts(&w.0, quoted, !quoted, f)?;
                }
            }
            ParamOp::Alternative(w) => {
                if is_set {
                    self.expand_parts(&w.0, quoted, !quoted, f)?;
                }
            }
            ParamOp::Assign(w) => {
                if is_set {
                    push_value(self, f);
                } else {
                    let ParamName::Var(name) = &pe.name else {
                        self.error(format!("{}: bad variable name", param_display(&pe.name)));
                        return Err(Flow::Error(2));
                    };
                    let v = self.expand_word_str(w)?;
                    self.set_var(name, v.clone())?;
                    push_result(&v, quoted, f);
                }
            }
            ParamOp::Error(w) => {
                if is_set {
                    push_value(self, f);
                } else {
                    let msg = if w.0.is_empty() {
                        if pe.colon {
                            "parameter null or not set".to_string()
                        } else {
                            "parameter not set".to_string()
                        }
                    } else {
                        String::from_utf8_lossy(&self.expand_word_str(w)?).into_owned()
                    };
                    return Err(self.unset_error(&pe.name, &msg));
                }
            }
            ParamOp::RemoveSmallestSuffix(w)
            | ParamOp::RemoveLargestSuffix(w)
            | ParamOp::RemoveSmallestPrefix(w)
            | ParamOp::RemoveLargestPrefix(w) => {
                if val.is_none() && nounset {
                    return Err(self.unset_error(&pe.name, "parameter not set"));
                }
                let how = match &pe.op {
                    ParamOp::RemoveSmallestSuffix(_) => Trim::SmallestSuffix,
                    ParamOp::RemoveLargestSuffix(_) => Trim::LargestSuffix,
                    ParamOp::RemoveSmallestPrefix(_) => Trim::SmallestPrefix,
                    _ => Trim::LargestPrefix,
                };
                let pat = self.expand_pattern(w)?;
                let v = pattern::trim(&val.unwrap_or_default(), &pat, how);
                push_result(&v, quoted, f);
            }
            ParamOp::Length => unreachable!(),
        }
        Ok(())
    }

    /// `$@`, `$*`, `"$@"`, and `"$*"`.
    fn push_positional(&mut self, at: bool, quoted: bool, f: &mut Fields) {
        let params = &self.positional;
        if f.splitting() && (at || !quoted) {
            for (i, p) in params.iter().enumerate() {
                if quoted {
                    if i > 0 {
                        f.finish();
                    }
                    f.push_quoted(p);
                } else {
                    if i > 0 {
                        f.break_field();
                    }
                    f.push_expansion(p);
                }
            }
            return;
        }
        let sep = match self.vars.get(b"IFS") {
            _ if at => vec![b' '],
            None => vec![b' '],
            Some(ifs) => ifs.iter().take(1).copied().collect(),
        };
        let joined = params.join(&sep[..]);
        push_result(&joined, quoted, f);
    }

    /// Runs `$(...)` in a subshell and returns its output without trailing
    /// newlines.
    pub fn command_subst(&mut self, list: &List) -> EResult<Vec<u8>> {
        let (r, w) = match sys::pipe() {
            Ok(p) => p,
            Err(e) => {
                self.error(format!("Pipe call failed: {}", sys::strerror(e)));
                return Err(Flow::Error(2));
            }
        };
        let pid = self.fork_or_error()?;
        if pid == 0 {
            sys::close(r);
            if w != 1 {
                let _ = sys::dup2(w, 1);
                sys::close(w);
            }
            let res = self.run_list(list);
            self.child_exit(res);
        }
        sys::close(w);
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = sys::read(r, &mut buf, false) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        sys::close(r);
        let status = self.wait_for(pid);
        self.subst_status = Some(status);
        while out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }
}

fn push_result(s: &[u8], quoted: bool, f: &mut Fields) {
    if quoted {
        f.push_quoted(s);
    } else {
        f.push_expansion(s);
    }
}

fn param_display(name: &ParamName) -> String {
    match name {
        ParamName::Var(n) => String::from_utf8_lossy(n).into_owned(),
        ParamName::Positional(n) => n.to_string(),
        ParamName::Special(c) => (*c as char).to_string(),
    }
}
