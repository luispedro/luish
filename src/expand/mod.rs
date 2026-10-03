//! Word expansion (XCU §2.6): tilde, parameter, command substitution,
//! arithmetic, field splitting, pathname expansion, and quote removal,
//! after brace expansion (`brace.rs`) when it is on.

pub mod arith;
pub mod brace;
pub mod glob;
pub mod modify;
pub mod pattern;
pub mod qual;
pub mod split;

use crate::ast::*;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;
use crate::vars::{Item, Subscript, Value};
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
    /// So are those of `setopt`, whose names can have `.` in them.
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
            if let Some(Some(is_name)) = decl
                && let Some(arg) = self.expand_plain_declaration(w, is_name)?
            {
                out.push(arg);
            } else if let Some(Some(is_name)) = decl
                && let Some(a) = crate::parser::split_assignment_with(w, is_name)
                && a.index.is_none()
                && !a.append
            {
                let Assign {
                    name: mut arg, value, ..
                } = a;
                arg.push(b'=');
                match value.0.as_slice() {
                    // Passed as a NUL and each element, `=value`, with its
                    // key before it as `[key`, each followed by a NUL (see
                    // `builtins::vars::split_arg`).
                    [WordPart::Array(items)] => {
                        arg.push(0);
                        for item in self.expand_items(items)? {
                            if let Some(k) = item.key {
                                arg.push(b'[');
                                arg.extend(k);
                                arg.push(0);
                            }
                            arg.push(b'=');
                            arg.extend(item.value);
                            arg.push(0);
                        }
                    }
                    _ => arg.extend(self.expand_word_str(&value)?),
                }
                out.push(arg);
            } else {
                self.expand_word_into(w, &mut out)?;
            }
        }
        Ok(out)
    }

    /// The common case of an argument of a declaration command, `name=value`
    /// with no tilde to expand in the value and no array (as in `local
    /// x="$1"`), expanded without building the assignment's word (as
    /// `split_assignment_with` does, copying its parts). `None` for others.
    fn expand_plain_declaration(&mut self, w: &Word, is_name: fn(&[u8]) -> bool) -> EResult<Option<Vec<u8>>> {
        let Some((WordPart::Literal(s), tail)) = w.0.split_first() else {
            return Ok(None);
        };
        let Some(eq) = s.iter().position(|&c| c == b'=') else {
            return Ok(None);
        };
        let rest = &s[eq + 1..];
        let plain = |part: &WordPart| match part {
            WordPart::Literal(s) => !s.contains(&b'~'),
            // Its word gets tilde expansion (`mark_param_word_tildes`).
            WordPart::Param(pe) => !matches!(
                pe.op,
                ParamOp::Default(_) | ParamOp::Assign(_) | ParamOp::Error(_) | ParamOp::Alternative(_)
            ),
            WordPart::Array(_) => false,
            _ => true,
        };
        if rest.contains(&b'~') || !tail.iter().all(plain) || !is_name(&s[..eq]) {
            return Ok(None);
        }
        let mut arg = s[..=eq].to_vec();
        if tail.is_empty() {
            arg.extend_from_slice(rest);
            return Ok(Some(arg));
        }
        let mut f = Fields::new(None);
        f.push_literal(rest);
        self.expand_parts(tail, false, false, &mut f)?;
        if let Some(v) = f.into_fields().first() {
            arg.extend(bytes(v));
        }
        Ok(Some(arg))
    }

    /// The elements of an array, `(x [key]=value)`: an element is expanded
    /// as a command word (giving any number of them), and `[key]=value` as
    /// an assignment.
    pub fn expand_items(&mut self, items: &[ArrayItem]) -> EResult<Vec<Item>> {
        let mut out = Vec::with_capacity(items.len());
        let mut words = Vec::new();
        for item in items {
            match &item.key {
                Some(k) => out.push(Item {
                    key: Some(self.expand_word_str(k)?),
                    value: self.expand_word_str(&item.value)?,
                }),
                None => {
                    self.expand_word_into(&item.value, &mut words)?;
                    out.extend(words.drain(..).map(|value| Item { key: None, value }));
                }
            }
        }
        Ok(out)
    }

    /// The subscript of `name[w]`: a key (`w` expanded as a string) if
    /// `name` is an associative array, otherwise an index (`w` expanded
    /// and evaluated as arithmetic).
    pub fn subscript(&mut self, name: &[u8], w: &Word) -> EResult<Subscript> {
        if self.vars.is_assoc(name) {
            Ok(Subscript::Key(self.expand_word_str(w)?))
        } else {
            Ok(Subscript::Index(self.arith_word(w)?))
        }
    }

    fn expand_word_into(&mut self, w: &Word, out: &mut Vec<Vec<u8>>) -> EResult<()> {
        if self.opt(Opt::BraceExpand)
            && brace::has_brace(w)
            && let Some(words) = brace::expand(w, &mut |w| self.expand_word_str(w))?
        {
            for w in &words {
                self.expand_braceless(w, out)?;
            }
            return Ok(());
        }
        self.expand_braceless(w, out)
    }

    /// A word's fields, after brace expansion.
    fn expand_braceless(&mut self, w: &Word, out: &mut Vec<Vec<u8>>) -> EResult<()> {
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

    /// Expansion of a here-string's word: no splitting or globbing, and
    /// the fields of `"$@"` are joined with spaces, as in bash and zsh.
    pub fn expand_here_string(&mut self, w: &Word) -> EResult<Vec<u8>> {
        if let Some(lit) = w.as_literal() {
            return Ok(lit.to_vec());
        }
        let mut f = Fields::new(None);
        self.expand_parts(&w.0, false, false, &mut f)?;
        let fields: Vec<Vec<u8>> = f.into_fields().iter().map(|f| bytes(f)).collect();
        Ok(fields.join(&b' '))
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
        // Words nested in `${...}` and `"..."` recurse through here.
        self.check_stack()?;
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
            // Only in an argument of a declaration command, handled in
            // `expand_command_words`; elsewhere the elements as text.
            WordPart::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.push_quoted(b" ");
                    }
                    if let Some(k) = &item.key {
                        let k = self.expand_word_str(k)?;
                        f.push_quoted(b"[");
                        f.push_quoted(&k);
                        f.push_quoted(b"]=");
                    }
                    let v = self.expand_word_str(&item.value)?;
                    f.push_quoted(&v);
                }
            }
            WordPart::DoubleQuoted(inner) => {
                // A quoted word is a field even when it expands to nothing,
                // except a lone "$@" with no positional parameters.
                let lone_at = matches!(inner.as_slice(), [WordPart::Param(pe)] if pe.flags.is_some() || is_list(pe));
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
            WordPart::ProcSubst { output, list } => {
                let path = self.process_subst(*output, list)?;
                f.push_quoted(&path);
            }
            WordPart::Arith(w) => {
                // Not `arith_word`, which is slower here, out of line.
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
        self.check_stack()?;
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
            ParamName::Indirect(_) => unreachable!(),
        }
    }

    fn unset_error(&self, name: &ParamName, msg: &str) -> Flow {
        self.error(format!("{}: {msg}", param_display(name)));
        Flow::Error(2)
    }

    fn expand_param(&mut self, pe: &ParamExp, quoted: bool, f: &mut Fields) -> EResult<()> {
        // The common case, a plain `$name` that is set, without copying the
        // value.
        if let (ParamName::Var(n), ParamOp::Plain, None, None) = (&pe.name, &pe.op, &pe.index, &pe.flags)
            && n != b"LINENO"
            && let Some(v) = self.vars.get(n)
        {
            push_result(v, quoted, f);
            return Ok(());
        }
        if let Some(flags) = &pe.flags {
            return self.expand_flagged(pe, flags, quoted, f);
        }
        self.expand_unflagged(pe, quoted, f)
    }

    fn expand_unflagged(&mut self, pe: &ParamExp, quoted: bool, f: &mut Fields) -> EResult<()> {
        if let ParamName::Indirect(base) = &pe.name {
            return self.expand_indirect(pe, base, quoted, f);
        }
        let multi = matches!(pe.name, ParamName::Special(b'@' | b'*'));
        // `${a[i]}` is an element, `${a[@]}` and `${a[*]}` the list. `$@`
        // and `$*` are joined only for `${@#pat}` and the like; the other
        // operators use the list.
        let (val, element) = match (&pe.index, &pe.name) {
            (None, _) if multi && !is_trim(&pe.op) => (None, None),
            (None, _) => (self.param_value(&pe.name), None),
            (Some(Index::Expr(w)), ParamName::Var(name)) => {
                let sub = self.subscript(name, w)?;
                (self.element(name, &sub), Some(sub))
            }
            (Some(Index::Slice(s)), ParamName::Var(name)) if self.vars.is_assoc(name) => {
                // The key `i..j`, which gives a word in double quotes even
                // if it isn't set (`is_list` can't tell).
                f.cur_exists |= quoted;
                let sub = Subscript::Key(self.slice_key(s)?);
                (self.element(name, &sub), Some(sub))
            }
            (Some(Index::Slice(s)), ParamName::Var(name)) => {
                let items = self.slice(name, s)?;
                return self.array_op(pe, name, items, true, quoted, f);
            }
            (Some(index), ParamName::Var(name)) => {
                return self.expand_array(pe, name, *index == Index::At, quoted, f);
            }
            _ => unreachable!(),
        };
        let unset_error = |sh: &Shell, msg: &str| match &element {
            Some(sub) => {
                sh.error(format!("{}[{sub}]: {msg}", param_display(&pe.name)));
                Flow::Error(2)
            }
            None => sh.unset_error(&pe.name, msg),
        };
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
                    None if nounset => return Err(unset_error(self, "parameter not set")),
                    None => 0,
                }
            };
            push_result(n.to_string().as_bytes(), quoted, f);
            return Ok(());
        }
        if let ParamOp::Substring(..) | ParamOp::Replace(..) | ParamOp::Modify(_) = pe.op {
            if val.is_none() && nounset {
                return Err(unset_error(self, "parameter not set"));
            }
            return self.expand_slice_op(pe, val, multi, quoted, f);
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
                    return Err(unset_error(self, "parameter not set"));
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
                    match &element {
                        Some(sub) => self.set_element(name, sub, v.clone(), false)?,
                        None => self.set_var(name, v.clone())?,
                    }
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
                    return Err(unset_error(self, &msg));
                }
            }
            ParamOp::RemoveSmallestSuffix(w)
            | ParamOp::RemoveLargestSuffix(w)
            | ParamOp::RemoveSmallestPrefix(w)
            | ParamOp::RemoveLargestPrefix(w) => {
                if val.is_none() && nounset {
                    return Err(unset_error(self, "parameter not set"));
                }
                let how = trim_kind(&pe.op);
                let pat = self.expand_pattern(w)?;
                let v = val.unwrap_or_default();
                push_result(pattern::trim(&v, &pat, how), quoted, f);
            }
            ParamOp::Length
            | ParamOp::Keys
            | ParamOp::Names
            | ParamOp::Substring(..)
            | ParamOp::Replace(..)
            | ParamOp::Modify(_) => unreachable!(),
            ParamOp::Bad(_) => {
                self.error("Bad substitution");
                return Err(Flow::Error(2));
            }
        }
        Ok(())
    }

    /// `${!name...}` (bash): the operator applies to the parameter named
    /// by the value of `name` (or of the element `${!name[i]}`), which can
    /// be a variable, `name[index]`, a positional or a special parameter.
    fn expand_indirect(&mut self, pe: &ParamExp, base: &ParamName, quoted: bool, f: &mut Fields) -> EResult<()> {
        if let ParamOp::Bad(_) = pe.op {
            self.error("Bad substitution");
            return Err(Flow::Error(2));
        }
        let reference = match (&pe.index, base) {
            (Some(Index::Expr(w)), ParamName::Var(name)) => {
                let sub = self.subscript(name, w)?;
                self.element(name, &sub)
            }
            _ => self.param_value(base),
        };
        let Some(reference) = reference else {
            // As in bash, also without `set -u`.
            self.error(format!("{}: invalid indirect expansion", param_display(base)));
            return Err(Flow::Error(2));
        };
        let Some((name, index)) = parse_reference(&reference) else {
            self.error(format!(
                "{}: invalid variable name",
                String::from_utf8_lossy(&reference)
            ));
            return Err(Flow::Error(2));
        };
        let target = ParamExp {
            name,
            index,
            op: pe.op.clone(),
            colon: pe.colon,
            flags: None,
        };
        // A lone `"${!x}"` gives no field if `x` names `@` or `a[@]` (see
        // `is_list`).
        if quoted && !is_list(&target) {
            f.cur_exists = true;
        }
        self.expand_param(&target, quoted, f)
    }

    /// zsh's `${(flags)name...}`. The parameter, with its operator, is
    /// expanded as in double quotes into a list of words (as `"$@"` for a
    /// list); the flags then join, split, convert and order the words, in
    /// zsh's order.
    fn expand_flagged(&mut self, pe: &ParamExp, fl: &Flags, quoted: bool, f: &mut Fields) -> EResult<()> {
        // As in zsh, `$*` and `${a[*]}` are lists like `$@`, unless quoted
        // without `@` or `j`: then their elements are joined first, as are
        // those of `$@` and `${a[@]}` where the result is one word (in an
        // assignment, for example).
        let field_ctx = f.field_context();
        let separate = |at: bool| fl.join.is_some() || field_ctx && (at || fl.at || !quoted);
        let mut inner = Fields::new(Some(IfsSet::new(b"")));
        match (&pe.name, &pe.index) {
            (ParamName::Var(name), Some(index @ (Index::At | Index::Star))) => {
                let items = match self.vars.get_value(name) {
                    Some(Value::Assoc(h)) if fl.keys && fl.values => Some(
                        (h.keys().iter().zip(h.values()))
                            .flat_map(|(k, v)| [k.clone(), v.clone()])
                            .collect(),
                    ),
                    Some(Value::Assoc(h)) if fl.keys => Some(h.keys().to_vec()),
                    Some(v) => Some(v.elements().to_vec()),
                    None => self.special_elements(name),
                };
                self.array_op(pe, name, items, separate(*index == Index::At), true, &mut inner)?;
            }
            (ParamName::Var(name), Some(Index::Slice(s))) if !self.vars.is_assoc(name) => {
                let items = self.slice(name, s)?;
                self.array_op(pe, name, items, separate(true), true, &mut inner)?;
            }
            (ParamName::Special(c @ (b'@' | b'*')), None) if separate(*c == b'@') != (*c == b'@') => {
                let other = ParamExp {
                    name: ParamName::Special(if *c == b'@' { b'*' } else { b'@' }),
                    index: None,
                    op: pe.op.clone(),
                    colon: pe.colon,
                    flags: None,
                };
                self.expand_unflagged(&other, true, &mut inner)?;
            }
            _ => {
                if !is_list(pe) {
                    inner.cur_exists = true;
                }
                self.expand_unflagged(pe, true, &mut inner)?;
            }
        }
        let mut words: Vec<_> = inner.into_fields().iter().map(|w| bytes(w)).collect();
        // As in zsh, words are split only where the result can be several
        // words; they are joined first.
        let split = fl.split.as_deref().filter(|_| field_ctx);
        if let Some(sep) = &fl.join {
            words = vec![words.join(&sep[..])];
        } else if split.is_some() && words.len() != 1 {
            let sep: Vec<u8> = self.ifs_first().into_iter().collect();
            words = vec![words.join(&sep[..])];
        }
        if let Some(sep) = split {
            words = split_on(&words[0], sep);
            if quoted && !fl.at && words.len() > 2 {
                // As in zsh, only the first and the last word can be empty.
                let last = words.len() - 1;
                let mut i = 0;
                words.retain(|w| {
                    i += 1;
                    i == 1 || i - 1 == last || !w.is_empty()
                });
            }
        }
        if let Some(case) = fl.case {
            words.iter_mut().for_each(|w| change_case(w, case));
        }
        if fl.unique {
            crate::vars::dedupe(&mut words);
        }
        if fl.array_order {
            if fl.reverse {
                words.reverse();
            }
        } else if fl.sort || fl.reverse || fl.nocase || fl.numeric {
            // Stable, also in reverse.
            words.sort_by(|a, b| {
                let o = compare_words(a, b, fl.nocase, fl.numeric);
                if fl.reverse { o.reverse() } else { o }
            });
        }
        if split.is_some() && !quoted {
            // As in zsh, the words aren't split again, but empty ones are
            // dropped.
            for (i, w) in words.iter().filter(|w| !w.is_empty()).enumerate() {
                if i > 0 {
                    f.break_field();
                }
                f.push_literal(w);
            }
        } else {
            push_list(&words, true, quoted, self.ifs_first(), f);
        }
        Ok(())
    }

    /// An element of an array: an index counts from the end if it is
    /// negative, and a string is an array of one element.
    pub fn element(&self, name: &[u8], sub: &Subscript) -> Option<Vec<u8>> {
        let i = match sub {
            Subscript::Index(i) => *i,
            Subscript::Key(k) => match self.vars.get_value(name) {
                Some(Value::Assoc(h)) => return h.get(k).cloned(),
                _ => return None,
            },
        };
        let computed;
        let items = match self.vars.get_value(name) {
            Some(v) => v.elements(),
            None => {
                computed = self.special_elements(name)?;
                &computed[..]
            }
        };
        let i = if i < 0 { i + items.len() as i64 } else { i };
        usize::try_from(i).ok().and_then(|i| items.get(i)).cloned()
    }

    /// The elements of `${a[i..j]}` (`None` if `a` is unset), as Python
    /// slices them: from `i` (or the start) up to `j` (or the end), where
    /// an end counts from the end of the array if it is negative, and is
    /// clamped to the array.
    fn slice(&mut self, name: &[u8], s: &(Option<Word>, Option<Word>)) -> EResult<Option<Vec<Vec<u8>>>> {
        // The ends first, which may change the array.
        let start = s.0.as_ref().map(|w| self.arith_word(w)).transpose()?;
        let end = s.1.as_ref().map(|w| self.arith_word(w)).transpose()?;
        let computed;
        let items = match self.vars.get_value(name) {
            Some(v) => v.elements(),
            None => match self.special_elements(name) {
                Some(v) => {
                    computed = v;
                    &computed[..]
                }
                None => return Ok(None),
            },
        };
        let n = items.len() as i64;
        let clamp = |i: i64| (if i < 0 { i.saturating_add(n) } else { i }).clamp(0, n) as usize;
        let start = start.map_or(0, clamp);
        let end = end.map_or(items.len(), clamp).max(start);
        Ok(Some(items[start..end].to_vec()))
    }

    /// The key `i..j` of `${h[i..j]}`, for an associative array.
    fn slice_key(&mut self, s: &(Option<Word>, Option<Word>)) -> EResult<Vec<u8>> {
        let mut key = match &s.0 {
            Some(w) => self.expand_word_str(w)?,
            None => Vec::new(),
        };
        key.extend_from_slice(b"..");
        if let Some(w) = &s.1 {
            key.extend(self.expand_word_str(w)?);
        }
        Ok(key)
    }

    /// `${a[@]}` and `${a[*]}` (`at` tells which), with their operators,
    /// which apply to the list or to each element. A string is an array of
    /// one element.
    fn expand_array(&mut self, pe: &ParamExp, name: &[u8], at: bool, quoted: bool, f: &mut Fields) -> EResult<()> {
        if pe.op == ParamOp::Keys {
            // As in bash, an unset array has no keys, even with `set -u`.
            let keys = match self.vars.get_value(name) {
                Some(Value::Assoc(h)) => h.keys().to_vec(),
                Some(v) => (0..v.elements().len()).map(|i| i.to_string().into_bytes()).collect(),
                None => (0..self.special_elements(name).map_or(0, |v| v.len()))
                    .map(|i| i.to_string().into_bytes())
                    .collect(),
            };
            push_list(&keys, at, quoted, self.ifs_first(), f);
            return Ok(());
        }
        if pe.op == ParamOp::Names {
            // `name` is the prefix. As in `set`, specials (`RANDOM`) are
            // listed only once assigned (bash lists them all).
            let names: Vec<_> = (self.vars.sorted().into_iter())
                .filter(|(n, v)| v.value.is_some() && n.starts_with(name))
                .map(|(n, _)| n.to_vec())
                .collect();
            push_list(&names, at, quoted, self.ifs_first(), f);
            return Ok(());
        }
        // The common operators on a set array without copying all of it,
        // which would make `${#a[@]}` or `${a[@]:i:n}` in a loop quadratic.
        let sep = self.ifs_first();
        if let Some(v) = self.vars.get_value(name) {
            match &pe.op {
                ParamOp::Plain => {
                    push_list(v.elements(), at, quoted, sep, f);
                    return Ok(());
                }
                ParamOp::Length => {
                    push_result(v.elements().len().to_string().as_bytes(), quoted, f);
                    return Ok(());
                }
                ParamOp::Substring(offset, len) => {
                    // The offset and length first, which may change the
                    // array (as in bash, which slices the array as it is
                    // then).
                    let offset = self.arith_word(offset)?;
                    let len = len.as_ref().map(|w| self.arith_word(w)).transpose()?;
                    let n = self.vars.get_value(name).map_or(0, |v| v.elements().len());
                    let (start, end) = self.substring_range(n, offset, len)?;
                    if let Some(v) = self.vars.get_value(name) {
                        push_list(&v.elements()[start..end], at, quoted, sep, f);
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        let items = match self.vars.get_value(name) {
            Some(v) => Some(v.elements().to_vec()),
            None => self.special_elements(name),
        };
        self.array_op(pe, name, items, at, quoted, f)
    }

    /// The operator of `${a[@]}` or `${a[*]}` applied to the elements
    /// (`None` if the array is unset).
    fn array_op(
        &mut self,
        pe: &ParamExp,
        name: &[u8],
        items: Option<Vec<Vec<u8>>>,
        at: bool,
        quoted: bool,
        f: &mut Fields,
    ) -> EResult<()> {
        let sep = self.ifs_first();
        let unset = |sh: &Shell, msg: &str| {
            sh.error(format!(
                "{}[{}]: {msg}",
                String::from_utf8_lossy(name),
                if at { '@' } else { '*' }
            ));
            Flow::Error(2)
        };
        let conditional = matches!(
            pe.op,
            ParamOp::Default(_) | ParamOp::Alternative(_) | ParamOp::Assign(_) | ParamOp::Error(_)
        );
        if items.is_none() && !conditional && self.opt(Opt::Nounset) {
            return Err(unset(self, "parameter not set"));
        }
        // As for `$@`, null when the joined elements are.
        let is_set = items.as_ref().is_some_and(|items| {
            !pe.colon || items.iter().map(|v| v.len()).sum::<usize>() + items.len().saturating_sub(1) > 0
        });
        let items = items.unwrap_or_default();
        match &pe.op {
            ParamOp::Plain => push_list(&items, at, quoted, sep, f),
            ParamOp::Length => push_result(items.len().to_string().as_bytes(), quoted, f),
            ParamOp::Keys | ParamOp::Names => unreachable!(),
            ParamOp::Default(w) => {
                if is_set {
                    push_list(&items, at, quoted, sep, f);
                } else {
                    self.expand_parts(&w.0, quoted, !quoted, f)?;
                }
            }
            ParamOp::Alternative(w) => {
                if is_set {
                    self.expand_parts(&w.0, quoted, !quoted, f)?;
                }
            }
            ParamOp::Error(w) if !is_set => {
                let msg = if w.0.is_empty() {
                    if pe.colon {
                        "parameter null or not set"
                    } else {
                        "parameter not set"
                    }
                    .to_string()
                } else {
                    String::from_utf8_lossy(&self.expand_word_str(w)?).into_owned()
                };
                return Err(unset(self, &msg));
            }
            ParamOp::Error(_) => push_list(&items, at, quoted, sep, f),
            ParamOp::Assign(_) if !is_set => {
                self.error(format!("{}: bad variable name", String::from_utf8_lossy(name)));
                return Err(Flow::Error(2));
            }
            ParamOp::Assign(_) => push_list(&items, at, quoted, sep, f),
            ParamOp::RemoveSmallestSuffix(w)
            | ParamOp::RemoveLargestSuffix(w)
            | ParamOp::RemoveSmallestPrefix(w)
            | ParamOp::RemoveLargestPrefix(w) => {
                let how = trim_kind(&pe.op);
                let pat = self.expand_pattern(w)?;
                let items: Vec<_> = items.iter().map(|v| pattern::trim(v, &pat, how).to_vec()).collect();
                push_list(&items, at, quoted, sep, f);
            }
            ParamOp::Substring(offset, len) => {
                let offset = self.arith_word(offset)?;
                let len = len.as_ref().map(|w| self.arith_word(w)).transpose()?;
                let (start, end) = self.substring_range(items.len(), offset, len)?;
                push_list(&items[start..end], at, quoted, sep, f);
            }
            ParamOp::Replace(how, pat, rep) => {
                let pat = self.expand_pattern(pat)?;
                let rep = self.expand_word_str(rep)?;
                if quoted && !at {
                    // As in zsh, `"${a[*]/x/y}"` replaces in the joined string.
                    let sep: Vec<u8> = sep.into_iter().collect();
                    push_result(&pattern::replace(&items.join(&sep[..]), &pat, *how, &rep), quoted, f);
                } else {
                    let items: Vec<_> = items.iter().map(|v| pattern::replace(v, &pat, *how, &rep)).collect();
                    push_list(&items, at, quoted, sep, f);
                }
            }
            ParamOp::Modify(mods) => {
                if quoted && !at {
                    // As for `"${a[*]/x/y}"`, the joined string.
                    let sep: Vec<u8> = sep.into_iter().collect();
                    push_result(&modify_all(&items.join(&sep[..]), mods), quoted, f);
                } else {
                    let items: Vec<_> = items.iter().map(|v| modify_all(v, mods)).collect();
                    push_list(&items, at, quoted, sep, f);
                }
            }
            ParamOp::Bad(_) => {
                self.error("Bad substitution");
                return Err(Flow::Error(2));
            }
        }
        Ok(())
    }

    /// `$@`, `$*`, `"$@"`, and `"$*"`.
    fn push_positional(&mut self, at: bool, quoted: bool, f: &mut Fields) {
        push_list(&self.positional, at, quoted, self.ifs_first(), f);
    }

    /// `${x:offset:length}`, `${x/pattern/replacement}` and `${x:h}`, where
    /// `val` is the value of `x`. For `$@` and `$*`, they apply to the list of
    /// positional parameters and to each of them.
    fn expand_slice_op(
        &mut self,
        pe: &ParamExp,
        val: Option<Vec<u8>>,
        multi: bool,
        quoted: bool,
        f: &mut Fields,
    ) -> EResult<()> {
        let at = pe.name == ParamName::Special(b'@');
        match &pe.op {
            ParamOp::Substring(offset, len) => {
                let offset = self.arith_word(offset)?;
                let len = len.as_ref().map(|w| self.arith_word(w)).transpose()?;
                if multi {
                    // As in zsh, `$0` is included only from offset 0, and a
                    // negative offset counts from the end of the positional
                    // parameters only.
                    let mut items;
                    let list: &[Vec<u8>] = if offset == 0 {
                        items = vec![self.arg0.clone()];
                        items.extend(self.positional.iter().cloned());
                        &items
                    } else {
                        &self.positional
                    };
                    let offset = if offset > 0 { offset - 1 } else { offset };
                    let (start, end) = self.substring_range(list.len(), offset, len)?;
                    push_list(&list[start..end], at, quoted, self.ifs_first(), f);
                } else {
                    let v = val.unwrap_or_default();
                    let (start, end) = self.substring_range(v.len(), offset, len)?;
                    push_result(&v[start..end], quoted, f);
                }
            }
            ParamOp::Replace(how, pat, rep) => {
                let pat = self.expand_pattern(pat)?;
                let rep = self.expand_word_str(rep)?;
                if multi && quoted && !at {
                    // As in zsh, `"${*/a/b}"` replaces in the joined string.
                    let sep: Vec<u8> = self.ifs_first().into_iter().collect();
                    let joined = self.positional.join(&sep[..]);
                    push_result(&pattern::replace(&joined, &pat, *how, &rep), quoted, f);
                } else if multi {
                    let items: Vec<_> = (self.positional.iter())
                        .map(|p| pattern::replace(p, &pat, *how, &rep))
                        .collect();
                    push_list(&items, at, quoted, self.ifs_first(), f);
                } else {
                    let v = val.unwrap_or_default();
                    push_result(&pattern::replace(&v, &pat, *how, &rep), quoted, f);
                }
            }
            ParamOp::Modify(mods) => {
                if multi && quoted && !at {
                    let sep: Vec<u8> = self.ifs_first().into_iter().collect();
                    let joined = self.positional.join(&sep[..]);
                    push_result(&modify_all(&joined, mods), quoted, f);
                } else if multi {
                    let items: Vec<_> = self.positional.iter().map(|p| modify_all(p, mods)).collect();
                    push_list(&items, at, quoted, self.ifs_first(), f);
                } else {
                    push_result(&modify_all(&val.unwrap_or_default(), mods), quoted, f);
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// The range `${x:offset:length}` selects out of `n` bytes or elements.
    /// As in zsh, a negative offset counts from the end (and before the
    /// start it is the start), and a negative length leaves out that many
    /// at the end.
    fn substring_range(&self, n: usize, offset: i64, len: Option<i64>) -> EResult<(usize, usize)> {
        let n = n as i64;
        let start = if offset < 0 { (n + offset).max(0) } else { offset.min(n) };
        let end = match len {
            None => n,
            Some(l) if l >= 0 => start.saturating_add(l).min(n),
            Some(l) => {
                let end = n + l;
                if end < start {
                    self.error(format!("substring expression: {end} < {start}"));
                    return Err(Flow::Error(1));
                }
                end
            }
        };
        Ok((start as usize, end as usize))
    }

    /// Evaluates a word as an arithmetic expression, as in `$((...))`.
    pub fn arith_word(&mut self, w: &Word) -> EResult<i64> {
        let mut s = Vec::new();
        self.arith_text(&w.0, &mut s)?;
        arith::eval(self, &s).map_err(|msg| {
            self.error(msg);
            Flow::Error(2)
        })
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

/// Tells whether the text before the `=` of an argument is a name.
type NameTest = fn(&[u8]) -> bool;

/// Whether the command whose words have been expanded so far into `argv`
/// is `export`, `readonly`, `local` or `setopt`, possibly through `command`
/// (`None`: not known yet). If it is, the test for the names in its
/// assignments.
fn declaration_command(argv: &[Vec<u8>]) -> Option<Option<NameTest>> {
    let mut k = 0;
    loop {
        let name = argv.get(k)?;
        if name != b"command" {
            return Some(match &name[..] {
                b"export" | b"readonly" | b"local" | b"typeset" | b"declare" => Some(crate::lexer::is_valid_name),
                b"setopt" => Some(crate::options::is_setting_name),
                _ => None,
            });
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
                return Some(None);
            }
            k += 1;
        }
    }
}

fn trim_kind(op: &ParamOp) -> Trim {
    match op {
        ParamOp::RemoveSmallestSuffix(_) => Trim::SmallestSuffix,
        ParamOp::RemoveLargestSuffix(_) => Trim::LargestSuffix,
        ParamOp::RemoveSmallestPrefix(_) => Trim::SmallestPrefix,
        _ => Trim::LargestPrefix,
    }
}

/// Pushes a list of values, as `$@` (`at`) or `$*` pushes the positional
/// parameters. `sep` is the first character of IFS.
fn push_list(items: &[Vec<u8>], at: bool, quoted: bool, sep: Option<u8>, f: &mut Fields) {
    if f.field_context() && (at || !quoted) {
        for (i, p) in items.iter().enumerate() {
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
    let sep: Vec<u8> = sep.into_iter().collect();
    push_result(&items.join(&sep[..]), quoted, f);
}

/// The words of `${(s:sep:)x}`: an empty separator splits into bytes.
fn split_on(s: &[u8], sep: &[u8]) -> Vec<Vec<u8>> {
    if sep.is_empty() {
        return s.iter().map(|&c| vec![c]).collect();
    }
    let mut words = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + sep.len() <= s.len() {
        if &s[i..i + sep.len()] == sep {
            words.push(s[start..i].to_vec());
            i += sep.len();
            start = i;
        } else {
            i += 1;
        }
    }
    words.push(s[start..].to_vec());
    words
}

/// The `L`, `U` and `C` flags. Only ASCII letters change, as in `typeset -l`.
fn change_case(w: &mut [u8], case: Case) {
    match case {
        Case::Lower => w.make_ascii_lowercase(),
        Case::Upper => w.make_ascii_uppercase(),
        Case::Capitalize => {
            // A word is a run of letters and digits.
            let mut in_word = false;
            for c in w {
                *c = if in_word {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                };
                in_word = c.is_ascii_alphanumeric();
            }
        }
    }
}

/// The order of the `o` flag: bytes (after making ASCII letters lower
/// case, with `i`). With `n`, as in zsh, if the words differ first in a
/// number, they are ordered by its value, unless it is the same.
fn compare_words(a: &[u8], b: &[u8], nocase: bool, numeric: bool) -> std::cmp::Ordering {
    let lower;
    let (a, b) = if nocase {
        lower = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
        (&lower.0[..], &lower.1[..])
    } else {
        (a, b)
    };
    let digit = |s: &[u8], i: usize| s.get(i).is_some_and(u8::is_ascii_digit);
    let common = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    if numeric && (digit(a, common) || digit(b, common)) {
        let mut start = common;
        while start > 0 && a[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if digit(a, start) && digit(b, start) {
            fn number(s: &[u8]) -> &[u8] {
                let s = &s[..s.iter().take_while(|c| c.is_ascii_digit()).count()];
                &s[s.iter().take_while(|&&c| c == b'0').count()..]
            }
            let (x, y) = (number(&a[start..]), number(&b[start..]));
            let o = x.len().cmp(&y.len()).then(x.cmp(y));
            if o.is_ne() {
                return o;
            }
        }
    }
    a.cmp(b)
}

/// Applies zsh's modifiers in turn (`${x:A:h}`).
fn modify_all(v: &[u8], mods: &[modify::Modifier]) -> Vec<u8> {
    let mut v = v.to_vec();
    for &m in mods {
        v = modify::apply(&v, m);
    }
    v
}

/// Whether the operator removes a prefix or suffix (`${x#pat}` and the
/// like).
fn is_trim(op: &ParamOp) -> bool {
    matches!(
        op,
        ParamOp::RemoveSmallestSuffix(_)
            | ParamOp::RemoveLargestSuffix(_)
            | ParamOp::RemoveSmallestPrefix(_)
            | ParamOp::RemoveLargestPrefix(_)
    )
}

/// Whether `"${...}"` alone gives no field when there are no elements, as
/// `"$@"` does: `$@` and `${a[@]}`, also with a substring, replacement or
/// modifier (and a trim, for arrays).
fn is_list(pe: &ParamExp) -> bool {
    match (&pe.name, &pe.index) {
        (ParamName::Special(b'@'), None) => {
            matches!(
                pe.op,
                ParamOp::Plain | ParamOp::Substring(..) | ParamOp::Replace(..) | ParamOp::Modify(_)
            )
        }
        // Resolved in `expand_indirect`.
        (ParamName::Indirect(_), _) => true,
        (_, Some(Index::At | Index::Slice(_))) => matches!(
            pe.op,
            ParamOp::Plain
                | ParamOp::Keys
                | ParamOp::Names
                | ParamOp::Substring(..)
                | ParamOp::Replace(..)
                | ParamOp::Modify(_)
                | ParamOp::RemoveSmallestSuffix(_)
                | ParamOp::RemoveLargestSuffix(_)
                | ParamOp::RemoveSmallestPrefix(_)
                | ParamOp::RemoveLargestPrefix(_)
        ),
        _ => false,
    }
}

#[inline(always)]
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
        ParamName::Indirect(n) => format!("!{}", param_display(n)),
    }
}

/// The parameter that the value of `x` names in `${!x}`, as in bash: a
/// variable, `name[index]`, a positional parameter or a special one. As
/// in `unset 'a[i]'`, the index is not expanded, only evaluated (or taken
/// as the key of an associative array).
fn parse_reference(s: &[u8]) -> Option<(ParamName, Option<Index>)> {
    match s {
        [b'0'] => return Some((ParamName::Special(b'0'), None)),
        [c @ (b'@' | b'*' | b'#' | b'?' | b'-' | b'$' | b'!')] => return Some((ParamName::Special(*c), None)),
        _ if !s.is_empty() && s.iter().all(u8::is_ascii_digit) => {
            let n = std::str::from_utf8(s).ok()?.parse().unwrap_or(usize::MAX);
            return Some((ParamName::Positional(n), None));
        }
        _ => {}
    }
    let (name, index) = match s.split_last() {
        Some((b']', rest)) => {
            let open = rest.iter().position(|&c| c == b'[')?;
            let index = match &rest[open + 1..] {
                b"" => return None,
                b"@" => Index::At,
                b"*" => Index::Star,
                i => Index::Expr(Word(vec![WordPart::Literal(i.to_vec())])),
            };
            (&rest[..open], Some(index))
        }
        _ => (s, None),
    };
    crate::lexer::is_valid_name(name).then(|| (ParamName::Var(name.to_vec()), index))
}
