//! Word expansion (XCU §2.6): tilde, parameter, command substitution,
//! arithmetic, field splitting, pathname expansion, and quote removal.

pub mod arith;
pub mod glob;
pub mod pattern;
pub mod qual;
pub mod split;

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;
use pattern::{Trim, has_meta};
use split::{Fields, IfsSet, XChar, XField, bytes};

type EResult<T> = Result<T, Flow>;

impl Shell {
    fn ifs(&self) -> IfsSet {
        IfsSet::new(self.vars.get(b"IFS").unwrap_or(b" \t\n"))
    }

    /// The separator for `"$*"`: the first character of IFS, a space if
    /// IFS is unset, or none if it is empty.
    fn ifs_first(&self) -> Option<u8> {
        match self.vars.get(b"IFS") {
            None => Some(b' '),
            Some(ifs) => ifs.first().copied(),
        }
    }

    /// Full expansion of command words: produces zero or more fields each.
    pub fn expand_words(&mut self, words: &[Word]) -> EResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(words.len());
        for w in words {
            self.expand_word_into(w, &mut out)?;
        }
        Ok(out)
    }

    /// Expands the words of a simple command. As in dash, the arguments of
    /// `export`, `readonly` and `local` (also run through `command`) that
    /// have the form of an assignment are expanded as assignments: tilde
    /// expansion after `=` and `:`, and no field splitting or globbing.
    /// The command name is found by expanding the words one at a time.
    pub fn expand_command_words(&mut self, words: &[Word]) -> EResult<Vec<Vec<u8>>> {
        let mut out = Vec::with_capacity(words.len());
        let mut decl = None;
        let mut i = 0;
        while decl.is_none() && i < words.len() {
            self.expand_word_into(&words[i], &mut out)?;
            i += 1;
            decl = declaration_command(&out);
        }
        for w in &words[i..] {
            if decl == Some(true)
                && let Some(a) = crate::parser::split_assignment(w)
            {
                let mut arg = a.name;
                arg.push(b'=');
                arg.extend(self.expand_word_str(&a.value)?);
                out.push(arg);
            } else {
                self.expand_word_into(w, &mut out)?;
            }
        }
        Ok(out)
    }

    fn expand_word_into(&mut self, w: &Word, out: &mut Vec<Vec<u8>>) -> EResult<()> {
        if let Some(lit) = w.as_literal()
            && (self.opt(Opt::Noglob) || !lit.iter().any(|c| matches!(c, b'*' | b'?' | b'[')) || lit == b"[")
        {
            out.push(lit.to_vec());
            return Ok(());
        }
        let mut f = Fields::new(Some(self.ifs()));
        self.expand_parts(&w.0, false, false, &mut f)?;
        for field in f.into_fields() {
            self.glob_field(field, out)?;
        }
        Ok(())
    }

    fn glob_field(&mut self, field: XField, out: &mut Vec<Vec<u8>>) -> EResult<()> {
        if self.opt(Opt::Noglob) {
            out.push(bytes(&field));
            return Ok(());
        }
        if self.opt(Opt::Bareglobqual)
            && let Some(open) = qualifier_start(&field)
        {
            return self.glob_qualified(field, open, out);
        }
        if has_meta(&field) {
            let opts = glob::GlobOpts {
                globstar: self.opt(Opt::Globstar),
                dots: false,
            };
            let matches = glob::glob(&field, opts);
            if !matches.is_empty() {
                out.extend(matches);
                return Ok(());
            }
        }
        out.push(bytes(&field));
        Ok(())
    }

    /// A field ending in a glob qualifier, `pattern(q)` with the `(` at
    /// `open`: the pattern (even without `*`, `?` or `[`) is expanded, and
    /// the qualifier selects and changes the matches. Without matches the
    /// field is left as it was, unless the qualifier has `N`. As in zsh,
    /// the qualifier may come from an expansion.
    fn glob_qualified(&mut self, field: XField, open: usize, out: &mut Vec<Vec<u8>>) -> EResult<()> {
        let q = bytes(&field[open + 1..field.len() - 1]);
        let qual = match qual::Qualifiers::parse(&q) {
            Ok(qual) => qual,
            Err(msg) => {
                // Status 1, as in zsh.
                self.error(msg);
                return Err(Flow::Error(1));
            }
        };
        let opts = glob::GlobOpts {
            globstar: self.opt(Opt::Globstar),
            dots: qual.dots,
        };
        match qual.apply(glob::glob(&field[..open], opts)) {
            Some(matches) => out.extend(matches),
            None if !qual.null => out.push(bytes(&field)),
            None => {}
        }
        Ok(())
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
            // Recognized when the field is globbed (`glob_qualified`);
            // elsewhere it is just text.
            WordPart::GlobQual(q) => {
                f.push_literal(b"(");
                f.push_literal(q);
                f.push_literal(b")");
            }
            WordPart::DoubleQuoted(inner) => {
                // A quoted word is a field even when it expands to nothing,
                // except a lone "$@" with no positional parameters.
                let lone_at = matches!(inner.as_slice(), [WordPart::Param(pe)]
                    if pe.name == ParamName::Special(b'@') && matches!(pe.op, ParamOp::Plain));
                if !lone_at {
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
                let mut s = Vec::new();
                self.arith_text(&w.0, &mut s)?;
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

    /// The text of `$((...))` after expansion. As in dash, quotes and
    /// backslashes are kept, so the evaluator rejects them.
    fn arith_text(&mut self, parts: &[WordPart], out: &mut Vec<u8>) -> EResult<()> {
        for part in parts {
            match part {
                WordPart::DoubleQuoted(inner) => {
                    out.push(b'"');
                    self.arith_text(inner, out)?;
                    out.push(b'"');
                }
                WordPart::SingleQuoted(s) => {
                    out.push(b'\'');
                    out.extend_from_slice(s);
                    out.push(b'\'');
                }
                WordPart::Escaped(c) => {
                    out.push(b'\\');
                    out.push(*c);
                }
                WordPart::Literal(s) => out.extend_from_slice(s),
                _ => {
                    let mut f = Fields::new(None);
                    self.expand_part(part, false, false, &mut f)?;
                    if let Some(field) = f.into_fields().first() {
                        out.extend(bytes(field));
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
        // The common case, a plain `$name` that is set, without copying the
        // value.
        if let (ParamName::Var(n), ParamOp::Plain) = (&pe.name, &pe.op)
            && n != b"LINENO"
            && let Some(v) = self.vars.get(n)
        {
            push_result(v, quoted, f);
            return Ok(());
        }
        let multi = matches!(pe.name, ParamName::Special(b'@' | b'*'));
        let val = self.param_value(&pe.name);
        let nounset = self.opt(Opt::Nounset) && !multi;
        // dash: `$@` and `$*` always count as set; they are null when their
        // joined length (with separators, where there are any) is zero.
        let field_ctx = f.field_context();
        let multi_len = |sh: &Shell| {
            let ifs0 = sh.ifs_first();
            let sep = match pe.op {
                ParamOp::Alternative(_) | ParamOp::Length => ifs0.is_some(),
                _ => ifs0.is_some() || (field_ctx && (!quoted || pe.name == ParamName::Special(b'@'))),
            };
            let n = sh.positional.len();
            sh.positional.iter().map(|p| p.len()).sum::<usize>() + if sep { n.saturating_sub(1) } else { 0 }
        };
        if let ParamOp::Length = pe.op {
            let n = if multi {
                multi_len(self)
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
            _ if multi => !pe.colon || multi_len(self) > 0,
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
                let v = val.unwrap_or_default();
                push_result(pattern::trim(&v, &pat, how), quoted, f);
            }
            ParamOp::Length => unreachable!(),
            ParamOp::Bad(_) => {
                self.error("Bad substitution");
                return Err(Flow::Error(2));
            }
        }
        Ok(())
    }

    /// `$@`, `$*`, `"$@"`, and `"$*"`.
    fn push_positional(&mut self, at: bool, quoted: bool, f: &mut Fields) {
        if f.field_context() && (at || !quoted) {
            for (i, p) in self.positional.iter().enumerate() {
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
        // Outside a field context, dash joins `$@` like `$*`.
        let sep: Vec<u8> = self.ifs_first().into_iter().collect();
        let joined = self.positional.join(&sep[..]);
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
            // As in dash, a condition around `$(...)` doesn't suppress
            // `set -e` inside it.
            self.errexit_suppressed = 0;
            let res = self.run_list_exit(list, true);
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
        // As in dash, NUL bytes are dropped (they can't be in a C string).
        out.retain(|&b| b != 0);
        while out.last() == Some(&b'\n') {
            out.pop();
        }
        Ok(out)
    }
}

/// Where the glob qualifier of a field starts: the field ends in an
/// unquoted `(...)` that has something before it and no parenthesis inside.
fn qualifier_start(field: &[XChar]) -> Option<usize> {
    let (last, rest) = field.split_last()?;
    if last.quoted || last.b != b')' {
        return None;
    }
    let open = rest.iter().rposition(|c| !c.quoted && matches!(c.b, b'(' | b')'))?;
    (open > 0 && rest[open].b == b'(').then_some(open)
}

/// Whether the command whose words have been expanded so far into `argv`
/// is `export`, `readonly` or `local`, possibly through `command`
/// (`None`: not known yet).
fn declaration_command(argv: &[Vec<u8>]) -> Option<bool> {
    let mut k = 0;
    loop {
        let name = argv.get(k)?;
        if name != b"command" {
            return Some(matches!(&name[..], b"export" | b"readonly" | b"local"));
        }
        k += 1;
        // `command`'s options: only `-p` leaves a command to run.
        loop {
            let a = argv.get(k)?;
            if a == b"--" {
                k += 1;
                break;
            }
            if a.len() < 2 || a[0] != b'-' {
                break;
            }
            if !a[1..].iter().all(|&c| c == b'p') {
                return Some(false);
            }
            k += 1;
        }
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
