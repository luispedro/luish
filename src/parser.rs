//! Recursive-descent parser for the POSIX shell grammar (XCU §2.10).

use std::rc::Rc;

use crate::ast::*;
use crate::lexer::{AliasMap, Op, PResult, ParseError, Parser, Tok, Token, is_name_char, is_valid_name};

const RESERVED: &[&[u8]] = &[
    b"!",
    b"{",
    b"}",
    b"[[",
    b"]]",
    b"case",
    b"do",
    b"done",
    b"elif",
    b"else",
    b"esac",
    b"fi",
    b"for",
    b"function",
    b"if",
    b"in",
    b"then",
    b"until",
    b"while",
];

pub fn is_reserved(w: &[u8]) -> bool {
    RESERVED.contains(&w)
}

/// Words that end a compound list.
fn is_list_terminator(w: &[u8]) -> bool {
    matches!(
        w,
        b"then" | b"else" | b"elif" | b"fi" | b"do" | b"done" | b"esac" | b"}"
    )
}

impl Parser {
    /// Parses the next line of input (a list terminated by a newline or the
    /// end of input). Returns `None` at the end of input.
    pub fn parse_next(&mut self, aliases: &Rc<AliasMap>) -> PResult<Option<List>> {
        self.aliases = aliases.clone();
        loop {
            match self.peek()?.tok {
                Tok::Newline => {
                    self.next()?;
                }
                Tok::Eof => return Ok(None),
                _ => break,
            }
        }
        self.started = true;
        let list = self.parse_line_list()?;
        let t = self.next()?;
        match t.tok {
            Tok::Newline | Tok::Eof => Ok(Some(list)),
            _ => self.unexpected(&t, None),
        }
    }

    /// Parses all the input into a single list (used for backquotes).
    pub fn parse_all(&mut self) -> PResult<List> {
        let aliases = self.aliases.clone();
        let mut all = Vec::new();
        while let Some(list) = self.parse_next(&aliases)? {
            all.extend(list);
        }
        Ok(all)
    }

    pub(crate) fn unexpected<T>(&self, t: &Token, expecting: Option<&str>) -> PResult<T> {
        let mut msg = format!("Syntax error: {} unexpected", self.token_text(t));
        if let Some(e) = expecting {
            msg.push_str(&format!(" (expecting \"{e}\")"));
        }
        Err(ParseError {
            msg,
            lineno: t.lineno,
            incomplete: t.tok == Tok::Eof && !self.source_eof,
        })
    }

    fn peek_op(&mut self) -> PResult<Option<Op>> {
        Ok(match self.peek()?.tok {
            Tok::Op(op) => Some(op),
            _ => None,
        })
    }

    /// The next token as a plain literal word (a reserved-word candidate).
    fn peek_literal(&mut self) -> PResult<Option<Vec<u8>>> {
        Ok(match &self.peek()?.tok {
            Tok::Word(w) => w.as_literal().map(|s| s.to_vec()),
            _ => None,
        })
    }

    fn peek_is_kw(&mut self, kw: &[u8]) -> PResult<bool> {
        Ok(self.peek_literal()?.as_deref() == Some(kw))
    }

    fn expect_kw(&mut self, kw: &str) -> PResult<()> {
        let t = self.next()?;
        if let Tok::Word(w) = &t.tok
            && w.as_literal() == Some(kw.as_bytes())
        {
            return Ok(());
        }
        self.unexpected(&t, Some(kw))
    }

    fn expect_op(&mut self, op: Op) -> PResult<()> {
        let t = self.next()?;
        if t.tok == Tok::Op(op) {
            Ok(())
        } else {
            self.unexpected(&t, Some(op.text()))
        }
    }

    fn skip_newlines(&mut self) -> PResult<()> {
        while self.peek()?.tok == Tok::Newline {
            self.next()?;
        }
        Ok(())
    }

    /// `and_or ((';' | '&') and_or)* [';' | '&']` on one line.
    fn parse_line_list(&mut self) -> PResult<List> {
        let mut list = Vec::new();
        loop {
            let ao = self.parse_and_or()?;
            let async_ = match self.peek_op()? {
                Some(Op::Semi) => false,
                Some(Op::Amp) => true,
                _ => {
                    list.push(CompleteCommand {
                        list: ao,
                        async_: false,
                    });
                    break;
                }
            };
            self.next()?;
            list.push(CompleteCommand { list: ao, async_ });
            if matches!(self.peek()?.tok, Tok::Newline | Tok::Eof) {
                break;
            }
        }
        Ok(list)
    }

    /// A list of commands inside a compound command, ending at a reserved
    /// word such as `fi`, or at `)` or `;;`.
    pub(crate) fn parse_compound_list(&mut self) -> PResult<List> {
        let mut list = Vec::new();
        loop {
            self.skip_newlines()?;
            match &self.peek()?.tok {
                Tok::Op(Op::RParen | Op::DSemi) | Tok::Eof => break,
                Tok::Word(w) if w.as_literal().is_some_and(is_list_terminator) => break,
                _ => {}
            }
            let ao = self.parse_and_or()?;
            let async_ = match self.peek()?.tok {
                Tok::Op(Op::Semi) => false,
                Tok::Op(Op::Amp) => true,
                Tok::Newline => {
                    list.push(CompleteCommand {
                        list: ao,
                        async_: false,
                    });
                    continue;
                }
                _ => {
                    list.push(CompleteCommand {
                        list: ao,
                        async_: false,
                    });
                    break;
                }
            };
            self.next()?;
            list.push(CompleteCommand { list: ao, async_ });
        }
        Ok(list)
    }

    /// A compound list that must contain at least one command.
    fn parse_nonempty_list(&mut self) -> PResult<List> {
        let list = self.parse_compound_list()?;
        if list.is_empty() {
            let t = self.next()?;
            return self.unexpected(&t, None);
        }
        Ok(list)
    }

    fn parse_and_or(&mut self) -> PResult<AndOrList> {
        let first = self.parse_pipeline()?;
        let mut rest = Vec::new();
        loop {
            let kind = match self.peek_op()? {
                Some(Op::AndIf) => AndOr::And,
                Some(Op::OrIf) => AndOr::Or,
                _ => break,
            };
            self.next()?;
            self.skip_newlines()?;
            rest.push((kind, self.parse_pipeline()?));
        }
        Ok(AndOrList { first, rest })
    }

    fn parse_pipeline(&mut self) -> PResult<Pipeline> {
        let mut negated = false;
        while self.peek_is_kw(b"!")? {
            self.next()?;
            negated = !negated;
        }
        let mut cmds = vec![self.parse_command()?];
        while self.peek_op()? == Some(Op::Pipe) {
            self.next()?;
            self.skip_newlines()?;
            cmds.push(self.parse_command()?);
        }
        Ok(Pipeline { negated, cmds })
    }

    fn parse_command(&mut self) -> PResult<Command> {
        // Nested commands recurse through here (nested words through read_dollar).
        if !crate::stack::ok() {
            return self.err(crate::stack::TOO_DEEP);
        }
        self.maybe_expand_alias()?;
        if self.peek_op()? == Some(Op::LParen) {
            let cmd = self.parse_compound()?;
            let redirs = self.parse_redirects()?;
            return Ok(Command::Compound(cmd, redirs));
        }
        if let Some(w) = self.peek_literal()? {
            match w.as_slice() {
                b"{" | b"if" | b"while" | b"until" | b"for" | b"case" | b"[[" => {
                    let cmd = self.parse_compound()?;
                    let redirs = self.parse_redirects()?;
                    return Ok(Command::Compound(cmd, redirs));
                }
                b"function" => return self.parse_function_keyword(),
                w if is_reserved(w) => {
                    let t = self.next()?;
                    return self.unexpected(&t, None);
                }
                _ => {}
            }
        }
        self.parse_simple()
    }

    fn parse_compound(&mut self) -> PResult<CompoundCommand> {
        let t = self.next()?;
        if t.tok == Tok::Op(Op::LParen) {
            let list = self.parse_nonempty_list()?;
            self.expect_op(Op::RParen)?;
            return Ok(CompoundCommand::Subshell(list));
        }
        let kw = match &t.tok {
            Tok::Word(w) => w.as_literal().unwrap_or_default().to_vec(),
            _ => Vec::new(),
        };
        match kw.as_slice() {
            b"{" => {
                let list = self.parse_nonempty_list()?;
                self.expect_kw("}")?;
                Ok(CompoundCommand::BraceGroup(list))
            }
            b"if" => {
                let mut conds = Vec::new();
                let mut else_ = None;
                let cond = self.parse_nonempty_list()?;
                self.expect_kw("then")?;
                let body = self.parse_nonempty_list()?;
                conds.push((cond, body));
                loop {
                    let t = self.next()?;
                    match &t.tok {
                        Tok::Word(w) if w.as_literal() == Some(b"elif") => {
                            let cond = self.parse_nonempty_list()?;
                            self.expect_kw("then")?;
                            let body = self.parse_nonempty_list()?;
                            conds.push((cond, body));
                        }
                        Tok::Word(w) if w.as_literal() == Some(b"else") => {
                            else_ = Some(self.parse_nonempty_list()?);
                            self.expect_kw("fi")?;
                            break;
                        }
                        Tok::Word(w) if w.as_literal() == Some(b"fi") => break,
                        _ => return self.unexpected(&t, Some("fi")),
                    }
                }
                Ok(CompoundCommand::If { conds, else_ })
            }
            b"while" | b"until" => {
                let cond = self.parse_nonempty_list()?;
                self.expect_kw("do")?;
                let body = self.parse_nonempty_list()?;
                self.expect_kw("done")?;
                Ok(CompoundCommand::While {
                    cond,
                    body,
                    until: kw == b"until",
                })
            }
            b"for" => self.parse_for(t.lineno),
            b"case" => self.parse_case(t.lineno),
            b"[[" => {
                let expr = self.parse_cond_or()?;
                self.skip_newlines()?;
                let end = self.next()?;
                if !matches!(&end.tok, Tok::Word(w) if w.as_literal() == Some(b"]]")) {
                    return self.unexpected(&end, Some("]]"));
                }
                Ok(CompoundCommand::Cond { expr, lineno: t.lineno })
            }
            _ => self.unexpected(&t, None),
        }
    }

    // `[[ ... ]]`, as in zsh and bash. Its words aren't alias-expanded, and
    // newlines can come between any two of them, as in zsh. `<`, `>`, `(`,
    // `)`, `&&` and `||` are the lexer's operators; the rest are words,
    // recognized as operators only if unquoted.

    fn parse_cond_or(&mut self) -> PResult<CondExpr> {
        let mut e = self.parse_cond_and()?;
        loop {
            self.skip_newlines()?;
            if self.peek_op()? != Some(Op::OrIf) {
                return Ok(e);
            }
            self.next()?;
            e = CondExpr::Or(Box::new(e), Box::new(self.parse_cond_and()?));
        }
    }

    fn parse_cond_and(&mut self) -> PResult<CondExpr> {
        let mut e = self.parse_cond_not()?;
        loop {
            self.skip_newlines()?;
            if self.peek_op()? != Some(Op::AndIf) {
                return Ok(e);
            }
            self.next()?;
            e = CondExpr::And(Box::new(e), Box::new(self.parse_cond_not()?));
        }
    }

    fn parse_cond_not(&mut self) -> PResult<CondExpr> {
        self.skip_newlines()?;
        if self.peek_is_kw(b"!")? {
            self.next()?;
            return Ok(CondExpr::Not(Box::new(self.parse_cond_not()?)));
        }
        let t = self.next()?;
        let w = match t.tok {
            Tok::Op(Op::LParen) => {
                let e = self.parse_cond_or()?;
                self.skip_newlines()?;
                self.expect_op(Op::RParen)?;
                return Ok(e);
            }
            Tok::Word(w) if w.as_literal() != Some(b"]]") => w,
            _ => return self.unexpected(&t, None),
        };
        let unary = match w.as_literal() {
            Some([b'-', op]) if COND_UNARY.contains(op) => Some(*op),
            _ => None,
        };
        self.skip_newlines()?;
        let op = match &self.peek()?.tok {
            Tok::Op(Op::Less) => Some(CondOp::Less),
            Tok::Op(Op::Great) => Some(CondOp::Greater),
            Tok::Word(o) if unary.is_none() => o.as_literal().and_then(CondOp::from_text),
            _ => None,
        };
        if let Some(u) = unary
            && op.is_none()
        {
            // A unary operator takes the next word (so `[[ -n ]]` is an
            // error), unless it is a binary operator with a word after it:
            // as in zsh, `[[ -n = x ]]` compares strings.
            let x = self.cond_operand(false)?;
            if let Some(op) = x.as_literal().and_then(CondOp::from_text)
                && let Some(rhs) = self.cond_operand_opt(op == CondOp::Regex)?
            {
                return Ok(CondExpr::Binary(op, w, rhs));
            }
            return Ok(CondExpr::Unary(u, x));
        }
        let Some(op) = op else {
            return Ok(CondExpr::Unary(b'n', w));
        };
        self.next()?;
        let rhs = self.cond_operand(op == CondOp::Regex)?;
        Ok(CondExpr::Binary(op, w, rhs))
    }

    /// The word after an operator. After `=~` (`regex`), `(` and `|` are
    /// part of the word, and so are blanks and `<`, `>`, `;` and `&`
    /// inside parentheses, as in bash, so `[[ $x =~ ^(a|b c)$ ]]` needs
    /// no quotes.
    fn cond_operand(&mut self, regex: bool) -> PResult<Word> {
        match self.cond_operand_opt(regex)? {
            Some(w) => Ok(w),
            None => {
                let t = self.next()?;
                self.unexpected(&t, None)
            }
        }
    }

    fn cond_operand_opt(&mut self, regex: bool) -> PResult<Option<Word>> {
        self.regex_word = regex;
        let r = self.skip_newlines().and_then(|_| self.peek().cloned());
        self.regex_word = false;
        match r?.tok {
            Tok::Word(w) if w.as_literal() != Some(b"]]") => {
                self.next()?;
                Ok(Some(w))
            }
            _ => Ok(None),
        }
    }

    /// After an alias whose value ends in a blank, the next word is checked
    /// for aliases too, wherever it is (as in dash, also a `for` variable or
    /// `in`).
    fn alias_continuation(&mut self) -> PResult<()> {
        if let Some(end) = self.alias_blank_end
            && self.peek()?.start >= end
        {
            self.alias_blank_end = None;
            self.maybe_expand_alias()?;
        }
        Ok(())
    }

    fn parse_for(&mut self, lineno: u32) -> PResult<CompoundCommand> {
        self.alias_continuation()?;
        let t = self.next()?;
        let var = match &t.tok {
            Tok::Word(w) if w.as_literal().is_some_and(is_valid_name) => w.as_literal().unwrap().to_vec(),
            Tok::Word(_) => return self.err("Syntax error: Bad for loop variable"),
            _ => return self.unexpected(&t, None),
        };
        let mut words = None;
        self.skip_newlines()?;
        self.alias_continuation()?;
        if self.peek_is_kw(b"in")? {
            self.next()?;
            let mut ws = Vec::new();
            loop {
                self.alias_continuation()?;
                let t = self.next()?;
                match t.tok {
                    Tok::Word(w) => ws.push(w),
                    Tok::Op(Op::Semi) | Tok::Newline => break,
                    _ => return self.unexpected(&t, Some("do")),
                }
            }
            words = Some(ws);
            self.skip_newlines()?;
        } else if self.peek_op()? == Some(Op::Semi) {
            self.next()?;
            self.skip_newlines()?;
        }
        self.expect_kw("do")?;
        let body = self.parse_nonempty_list()?;
        self.expect_kw("done")?;
        Ok(CompoundCommand::For {
            var,
            words,
            body,
            lineno,
        })
    }

    fn parse_case(&mut self, lineno: u32) -> PResult<CompoundCommand> {
        self.alias_continuation()?;
        let t = self.next()?;
        let Tok::Word(word) = t.tok else {
            return self.unexpected(&t, None);
        };
        self.skip_newlines()?;
        self.alias_continuation()?;
        self.expect_kw("in")?;
        let mut arms = Vec::new();
        loop {
            self.skip_newlines()?;
            if self.peek_is_kw(b"esac")? {
                self.next()?;
                break;
            }
            if self.peek_op()? == Some(Op::LParen) {
                self.next()?;
            }
            let mut patterns = Vec::new();
            loop {
                let t = self.next()?;
                match t.tok {
                    Tok::Word(w) => patterns.push(w),
                    _ => return self.unexpected(&t, Some(")")),
                }
                match self.peek_op()? {
                    Some(Op::Pipe) => {
                        self.next()?;
                    }
                    _ => break,
                }
            }
            self.expect_op(Op::RParen)?;
            let body = self.parse_compound_list()?;
            arms.push(CaseArm { patterns, body });
            let t = self.next()?;
            match &t.tok {
                Tok::Op(Op::DSemi) => {}
                Tok::Word(w) if w.as_literal() == Some(b"esac") => break,
                _ => return self.unexpected(&t, Some(";;")),
            }
        }
        Ok(CompoundCommand::Case { word, arms, lineno })
    }

    fn parse_redirects(&mut self) -> PResult<Vec<Redirect>> {
        let mut redirs = Vec::new();
        while let Some(r) = self.try_parse_redirect()? {
            redirs.push(r);
        }
        Ok(redirs)
    }

    fn try_parse_redirect(&mut self) -> PResult<Option<Redirect>> {
        let fd = match self.peek()?.tok {
            Tok::IoNumber(n) => {
                self.next()?;
                Some(n)
            }
            Tok::Op(op) if op.redir_kind().is_some() => None,
            _ => return Ok(None),
        };
        let t = self.next()?;
        let Tok::Op(op) = t.tok else {
            return self.unexpected(&t, None);
        };
        let Some(kind) = op.redir_kind() else {
            return self.unexpected(&t, None);
        };
        let t = if kind == RedirKind::HereDoc {
            self.next_raw()?
        } else {
            self.next()?
        };
        let Tok::Word(word) = t.tok else {
            return self.unexpected(&t, None);
        };
        let target = if kind == RedirKind::HereDoc {
            // dash doesn't parse `$(` in a delimiter, so `(` is unexpected.
            if word
                .0
                .iter()
                .any(|p| matches!(p, WordPart::CmdSubst(_) | WordPart::Arith(_)))
            {
                return self.err("Syntax error: \"(\" unexpected");
            }
            let raw = self.src[t.start..t.end].to_vec();
            RedirTarget::HereDoc(self.push_heredoc(&raw, op == Op::DLessDash))
        } else {
            RedirTarget::Word(word)
        };
        Ok(Some(Redirect { fd, kind, target }))
    }

    fn parse_simple(&mut self) -> PResult<Command> {
        let lineno = self.peek()?.lineno;
        let mut assigns = Vec::new();
        let mut words: Vec<Word> = Vec::new();
        let mut redirs = Vec::new();
        loop {
            if let Some(r) = self.try_parse_redirect()? {
                redirs.push(r);
                continue;
            }
            if words.is_empty() {
                self.maybe_expand_alias()?;
            } else {
                self.alias_continuation()?;
            }
            if !matches!(self.peek()?.tok, Tok::Word(_)) {
                break;
            }
            let t = self.next()?;
            let end = t.end;
            let Tok::Word(w) = t.tok else { unreachable!() };
            if words.is_empty()
                && let Some(mut a) = split_assignment(&w)
            {
                if a.index.is_none() && a.value.0.is_empty() && self.array_follows(end)? {
                    a.value = Word(vec![WordPart::Array(self.parse_array()?)]);
                }
                assigns.push(a);
                continue;
            }
            // `local a=(x y)`: the array is part of the argument.
            if is_declaration(&words)
                && let Some(lit) = w.as_literal()
                && let Some(name) = lit.strip_suffix(b"=")
                && is_valid_name(name)
                && self.array_follows(end)?
            {
                let items = self.parse_array()?;
                words.push(Word(vec![WordPart::Literal(lit.to_vec()), WordPart::Array(items)]));
                continue;
            }
            if words.is_empty() && assigns.is_empty() && redirs.is_empty() && self.peek_op()? == Some(Op::LParen) {
                return self.parse_function(w);
            }
            words.push(w);
        }
        if assigns.is_empty() && words.is_empty() && redirs.is_empty() {
            let t = self.next()?;
            return self.unexpected(&t, None);
        }
        Ok(Command::Simple(SimpleCommand {
            assigns,
            words,
            redirs,
            lineno,
        }))
    }

    /// Whether a `(` follows right after the word that ends at `end`, as in
    /// an array assignment, `a=(x y)`.
    fn array_follows(&mut self, end: usize) -> PResult<bool> {
        let t = self.peek()?;
        Ok(t.tok == Tok::Op(Op::LParen) && t.start == end)
    }

    /// The elements of an array, `(a b c)`, which can span lines.
    fn parse_array(&mut self) -> PResult<Vec<Word>> {
        self.next()?;
        let mut items = Vec::new();
        loop {
            let t = self.next()?;
            match t.tok {
                Tok::Newline => {}
                Tok::Word(w) => items.push(w),
                Tok::Op(Op::RParen) => return Ok(items),
                _ => return self.unexpected(&t, Some(")")),
            }
        }
    }

    fn parse_function(&mut self, name: Word) -> PResult<Command> {
        // As in dash, special built-ins can't be redefined.
        let Some(name) = name
            .as_literal()
            .filter(|n| is_valid_name(n) && !matches!(crate::builtins::lookup(n), Some((_, true))))
        else {
            return self.err("Syntax error: Bad function name");
        };
        let name = name.to_vec();
        self.expect_op(Op::LParen)?;
        self.expect_op(Op::RParen)?;
        self.skip_newlines()?;
        // As in dash, the body can be any command (`f() echo hi`, or even
        // `f() g() { ...; }`); one that isn't compound is kept as `{ cmd; }`.
        let body = match self.parse_command()? {
            Command::Compound(cmd, redirs) => FunctionBody { cmd, redirs },
            cmd => FunctionBody {
                cmd: CompoundCommand::BraceGroup(vec![CompleteCommand {
                    list: AndOrList {
                        first: Pipeline {
                            negated: false,
                            cmds: vec![cmd],
                        },
                        rest: Vec::new(),
                    },
                    async_: false,
                }]),
                redirs: Vec::new(),
            },
        };
        Ok(Command::FunctionDef {
            names: vec![name],
            body: Rc::new(body),
        })
    }

    /// `function NAME... [()] compound-command`, as in zsh and bash. The
    /// names needn't be valid variable names (`function git-up`), and aren't
    /// alias-expanded. Several names are zsh's, and a body that starts after
    /// the first name with a reserved word such as `if` is bash's.
    fn parse_function_keyword(&mut self) -> PResult<Command> {
        self.next()?;
        let mut names = Vec::new();
        loop {
            let Some(w) = self.peek_literal()? else {
                if matches!(self.peek()?.tok, Tok::Word(_)) {
                    return self.err("Syntax error: Bad function name");
                }
                break;
            };
            if w == b"{" || !names.is_empty() && matches!(&w[..], b"if" | b"while" | b"until" | b"for" | b"case") {
                break;
            }
            // As for `f()`, special built-ins can't be redefined; a name with
            // `/` would be run as a file.
            if w.contains(&b'/') || matches!(crate::builtins::lookup(&w), Some((_, true))) {
                return self.err("Syntax error: Bad function name");
            }
            self.next()?;
            names.push(w);
        }
        if names.is_empty() {
            let t = self.next()?;
            return self.unexpected(&t, None);
        }
        if self.peek_op()? == Some(Op::LParen) {
            self.next()?;
            if self.peek_op()? != Some(Op::RParen) {
                // `function f (cmd)`, a subshell body, as in bash.
                let list = self.parse_nonempty_list()?;
                self.expect_op(Op::RParen)?;
                let redirs = self.parse_redirects()?;
                return Ok(Command::FunctionDef {
                    names,
                    body: Rc::new(FunctionBody {
                        cmd: CompoundCommand::Subshell(list),
                        redirs,
                    }),
                });
            }
            self.next()?;
        }
        self.skip_newlines()?;
        let compound = match self.peek()?.tok {
            Tok::Op(Op::LParen) => true,
            _ => self
                .peek_literal()?
                .is_some_and(|w| matches!(&w[..], b"{" | b"if" | b"while" | b"until" | b"for" | b"case")),
        };
        if !compound {
            let t = self.next()?;
            return self.unexpected(&t, None);
        }
        let cmd = self.parse_compound()?;
        let redirs = self.parse_redirects()?;
        Ok(Command::FunctionDef {
            names,
            body: Rc::new(FunctionBody { cmd, redirs }),
        })
    }
}

/// Whether the command whose words have been parsed so far is `local`,
/// `export`, `readonly`, `typeset` or `declare` (also after `command` or
/// `builtin`), whose arguments can be arrays, `a=(x y)`.
fn is_declaration(words: &[Word]) -> bool {
    let mut names = words.iter().map(|w| w.as_literal());
    loop {
        match names.next() {
            Some(Some(b"command" | b"builtin")) => {}
            Some(Some(b"local" | b"export" | b"readonly" | b"typeset" | b"declare")) => return true,
            _ => return false,
        }
    }
}

/// Splits `NAME=value` into an assignment, if the word has that form.
pub(crate) fn split_assignment(w: &Word) -> Option<Assign> {
    split_assignment_with(w, is_valid_name)
}

/// [`split_assignment`] with another test for the name (for `setopt`).
/// Besides `NAME=value`, `NAME+=value` and `NAME[index]=value` (and `+=`)
/// are assignments when the name is a variable name.
pub(crate) fn split_assignment_with(w: &Word, is_name: fn(&[u8]) -> bool) -> Option<Assign> {
    let Some(WordPart::Literal(s)) = w.0.first() else {
        return None;
    };
    let assign = |name: &[u8], index, append, rest: &[u8], tail: &[WordPart]| {
        let mut parts = Vec::new();
        if !rest.is_empty() {
            parts.push(WordPart::Literal(rest.to_vec()));
        }
        parts.extend(tail.iter().cloned());
        Some(Assign {
            name: name.to_vec(),
            index,
            append,
            value: Word(crate::lexer::mark_assignment_tildes(parts)),
        })
    };
    if let Some(eq) = s.iter().position(|&c| c == b'=') {
        if is_name(&s[..eq]) {
            return assign(&s[..eq], None, false, &s[eq + 1..], &w.0[1..]);
        }
        if eq > 0 && s[eq - 1] == b'+' && is_valid_name(&s[..eq - 1]) {
            return assign(&s[..eq - 1], None, true, &s[eq + 1..], &w.0[1..]);
        }
    }
    let n = s.iter().position(|&c| !is_name_char(c)).unwrap_or(s.len());
    if n == 0 || s.get(n) != Some(&b'[') || !is_valid_name(&s[..n]) {
        return None;
    }
    // `NAME[index]=`: the index runs to the matching `]`, which must be
    // unquoted text followed by `=` or `+=`.
    let mut index = Vec::new();
    let mut depth = 0usize;
    let mut lit = Vec::new();
    for (k, part) in w.0.iter().enumerate() {
        let WordPart::Literal(text) = part else {
            if !lit.is_empty() {
                index.push(WordPart::Literal(std::mem::take(&mut lit)));
            }
            index.push(part.clone());
            continue;
        };
        let from = if k == 0 { n + 1 } else { 0 };
        for (i, &c) in text.iter().enumerate().skip(from) {
            match c {
                b'[' => depth += 1,
                b']' if depth > 0 => depth -= 1,
                b']' => {
                    let rest = &text[i + 1..];
                    let (append, rest) = match rest {
                        [b'=', rest @ ..] => (false, rest),
                        [b'+', b'=', rest @ ..] => (true, rest),
                        _ => return None,
                    };
                    if !lit.is_empty() {
                        index.push(WordPart::Literal(std::mem::take(&mut lit)));
                    }
                    return assign(&s[..n], Some(Word(index)), append, rest, &w.0[k + 1..]);
                }
                _ => {}
            }
            lit.push(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> List {
        let mut p = Parser::new(s.as_bytes().to_vec(), 1, true);
        p.parse_all().unwrap()
    }

    fn parse_err(s: &str) -> ParseError {
        let mut p = Parser::new(s.as_bytes().to_vec(), 1, true);
        p.parse_all().unwrap_err()
    }

    fn simple(l: &List) -> &SimpleCommand {
        match &l[0].list.first.cmds[0] {
            Command::Simple(s) => s,
            c => panic!("not simple: {c:?}"),
        }
    }

    #[test]
    fn simple_words() {
        let l = parse("echo a  'b c' \"d $x\"\n");
        let s = simple(&l);
        assert_eq!(s.words.len(), 4);
        assert_eq!(s.words[2], Word(vec![WordPart::SingleQuoted(b"b c".to_vec())]));
    }

    #[test]
    fn arrays() {
        let l = parse("a=(x \"y z\"\n $w) b+=(v) c[$i+1]=q d[2]+=r e+=s f[x]\n");
        let s = simple(&l);
        let a = &s.assigns[0];
        assert_eq!(
            (a.name.as_slice(), a.index.is_none(), a.append),
            (&b"a"[..], true, false)
        );
        assert_eq!(a.array().unwrap().len(), 3);
        assert!(s.assigns[1].append && s.assigns[1].array().is_some());
        let c = &s.assigns[2];
        assert_eq!(c.name, b"c");
        assert_eq!(c.index.as_ref().unwrap().0.len(), 2);
        assert_eq!(c.value, Word(vec![WordPart::Literal(b"q".to_vec())]));
        assert!(s.assigns[3].append && s.assigns[3].index.is_some());
        assert!(s.assigns[4].append && s.assigns[4].index.is_none());
        // Not an assignment: a command word.
        assert_eq!(s.words[0].as_literal(), Some(&b"f[x]"[..]));
        // Declaration commands take arrays as arguments.
        let l = parse("command local a=(x y) b=()\n");
        let s = simple(&l);
        assert!(matches!(s.words[2].0.last(), Some(WordPart::Array(items)) if items.len() == 2));
        assert!(matches!(s.words[3].0.last(), Some(WordPart::Array(items)) if items.is_empty()));
        // `(` after a blank, or after other words, is a syntax error.
        assert!(parse_err("a= (x)\n").msg.contains("\"(\" unexpected"));
        assert!(parse_err("echo a=(x)\n").msg.contains("\"(\" unexpected"));
        assert!(parse_err("a=(x;)\n").msg.contains("\";\" unexpected"));
    }

    #[test]
    fn assignments_and_redirs() {
        let l = parse("a=1 b=~/x cmd 2>&1 >out <<EOF\nhello $a\nEOF\n");
        let s = simple(&l);
        assert_eq!(s.assigns.len(), 2);
        assert_eq!(s.assigns[1].value.0[0], WordPart::Tilde(vec![]));
        assert_eq!(s.redirs.len(), 3);
        assert_eq!(s.redirs[0].fd, Some(2));
        let RedirTarget::HereDoc(hd) = &s.redirs[2].target else {
            panic!()
        };
        assert!(!hd.borrow().quoted);
    }

    #[test]
    fn case_in_cmdsubst() {
        let l = parse("echo $(case x in x) echo y;; esac)\n");
        let s = simple(&l);
        assert!(matches!(s.words[1].0[0], WordPart::CmdSubst(_)));
    }

    #[test]
    fn arith_vs_subshell() {
        let l = parse("echo $((1+2)) $( (echo x) )\n");
        let s = simple(&l);
        assert!(matches!(s.words[1].0[0], WordPart::Arith(_)));
        assert!(matches!(s.words[2].0[0], WordPart::CmdSubst(_)));
    }

    #[test]
    fn glob_qualifiers() {
        let qual = |s: &str| {
            let mut p = Parser::new(s.as_bytes().to_vec(), 1, true);
            p.bareglobqual = true;
            p.parse_all()
        };
        let l = qual("echo *(/) ~/x(N) \"a\"(.) $(echo *(@))\n").unwrap();
        let s = simple(&l);
        let last = |i: usize| s.words[i].0.last().unwrap().clone();
        assert_eq!(last(1), WordPart::GlobQual(b"/".to_vec()));
        assert_eq!(last(2), WordPart::GlobQual(b"N".to_vec()));
        assert!(matches!(s.words[2].0[0], WordPart::Tilde(_)));
        assert_eq!(last(3), WordPart::GlobQual(b".".to_vec()));
        // Function definitions and subshells are unchanged.
        for src in [
            "f() { :; }\n",
            "f( ) { :; }\n",
            "(echo *)\n",
            "case x in (*) :;; esac\n",
        ] {
            assert_eq!(qual(src).unwrap(), parse(src), "{src}");
        }
        // Arrays are not qualifiers.
        for src in ["a=(x y)\n", "a+=(x)\n", "local a=(x)\n"] {
            assert_eq!(qual(src).unwrap(), parse(src), "{src}");
        }
        // Not at the end of the word, or with a blank inside: an error.
        assert!(qual("echo *(/)x\n").is_err());
        assert!(qual("echo *(/ )\n").is_err());
        // Without the option, as in POSIX.
        assert!(parse_err("echo *(/)\n").msg.contains("\"(\" unexpected"));
    }

    #[test]
    fn heredocs_on_one_line() {
        let l = parse("cat <<A; cat <<'B'\na $x\nA\nb $x\nB\necho done\n");
        assert_eq!(l.len(), 3);
    }

    #[test]
    fn heredoc_in_cmdsubst() {
        let l = parse("x=$(cat <<EOF\nhi\nEOF\n)\necho $x\n");
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn compound() {
        let l = parse(
            "if a; then b; elif c; then d; else e; fi\nwhile x; do y; done\nfor i in 1 2; do :; done\nf() { echo; }\n",
        );
        assert_eq!(l.len(), 4);
        assert!(matches!(l[3].list.first.cmds[0], Command::FunctionDef { .. }));
    }

    #[test]
    fn function_keyword() {
        let def = |s: &str| match parse(s).remove(0).list.first.cmds.remove(0) {
            Command::FunctionDef { names, body } => (names, body.cmd.clone()),
            c => panic!("not a definition: {c:?}"),
        };
        let names = |s: &str| def(s).0;
        assert_eq!(names("function f { :; }\n"), [b"f"]);
        assert_eq!(names("function f() { :; }\n"), [b"f"]);
        assert_eq!(names("function f ( )\n\n{ :; }\n"), [b"f"]);
        assert_eq!(names("function a-b c.d { :; }\n"), [&b"a-b"[..], b"c.d"]);
        assert_eq!(names("function if { :; }\n"), [b"if"]);
        assert_eq!(names("function f\n{ :; }\n"), [b"f"]);
        assert!(matches!(def("function f (:)\n").1, CompoundCommand::Subshell(_)));
        assert!(matches!(
            def("function f if :; then :; fi\n").1,
            CompoundCommand::If { .. }
        ));
        // Not alias-expanded, and `function` isn't either.
        let mut aliases = AliasMap::default();
        aliases.insert(b"f".to_vec(), b"g".to_vec(), false);
        aliases.insert(b"function".to_vec(), b"echo".to_vec(), false);
        let mut p = Parser::new(b"function f { :; }\n".to_vec(), 1, true);
        let l = p.parse_next(&Rc::new(aliases)).unwrap().unwrap();
        assert!(matches!(&l[0].list.first.cmds[0], Command::FunctionDef { names, .. } if names == &[b"f"]));
        // Only as a command name.
        assert_eq!(simple(&parse("echo function\n")).words.len(), 2);
        for (src, msg) in [
            ("function f echo hi;\n", "Syntax error: \";\" unexpected"),
            ("function f() echo hi\n", "Syntax error: \"echo\" unexpected"),
            ("function\n", "Syntax error: newline unexpected"),
            ("function { :; }\n", "Syntax error: \"{\" unexpected"),
            ("function 'f' { :; }\n", "Syntax error: Bad function name"),
            ("function a/b { :; }\n", "Syntax error: Bad function name"),
            ("function export { :; }\n", "Syntax error: Bad function name"),
        ] {
            assert_eq!(parse_err(src).msg, msg, "{src}");
        }
    }

    #[test]
    fn cond() {
        let cond = |s: &str| match parse(s).remove(0).list.first.cmds.remove(0) {
            Command::Compound(CompoundCommand::Cond { expr, .. }, _) => expr,
            c => panic!("not [[: {c:?}"),
        };
        let lit = |s: &str| Word(vec![WordPart::Literal(s.as_bytes().to_vec())]);
        let un = |op, s| CondExpr::Unary(op, lit(s));
        assert_eq!(cond("[[ a ]]\n"), un(b'n', "a"));
        assert_eq!(cond("[[ -f a ]]\n"), un(b'f', "a"));
        // A unary operator takes the next word, whatever it is.
        assert_eq!(cond("[[ -n -z ]]\n"), un(b'n', "-z"));
        assert_eq!(cond("[[ -f = ]]\n"), un(b'f', "="));
        assert_eq!(
            cond("[[ -n = && a ]]\n"),
            CondExpr::And(Box::new(un(b'n', "=")), Box::new(un(b'n', "a")))
        );
        // Unless the word is a binary operator with a word after it.
        assert_eq!(
            cond("[[ -n = -n ]]\n"),
            CondExpr::Binary(CondOp::Match, lit("-n"), lit("-n"))
        );
        assert_eq!(
            cond("[[ -f < a ]]\n"),
            CondExpr::Binary(CondOp::Less, lit("-f"), lit("a"))
        );
        assert_eq!(cond("[[ a<b ]]\n"), CondExpr::Binary(CondOp::Less, lit("a"), lit("b")));
        assert_eq!(
            cond("[[ a == b ]]\n"),
            CondExpr::Binary(CondOp::Match, lit("a"), lit("b"))
        );
        // `&&` binds more tightly than `||`, and `!` than both.
        assert_eq!(
            cond("[[ ! a || b && ( c ) ]]\n"),
            CondExpr::Or(
                Box::new(CondExpr::Not(Box::new(un(b'n', "a")))),
                Box::new(CondExpr::And(Box::new(un(b'n', "b")), Box::new(un(b'n', "c"))))
            )
        );
        // Newlines anywhere, as in zsh.
        assert_eq!(cond("[[\n a\n =\n b\n ]]\n"), cond("[[ a = b ]]\n"));
        assert_eq!(cond("[[ a &&\n b ]]\n"), cond("[[ a && b ]]\n"));
        // After `=~`, `(` and `|` are part of the word, and so is anything
        // inside parentheses.
        assert_eq!(
            cond("[[ a =~ ^(x|y z)+|w$ ]]\n"),
            CondExpr::Binary(CondOp::Regex, lit("a"), lit("^(x|y z)+|w$"))
        );
        assert!(matches!(cond("[[ a =~ (x) && b ]]\n"), CondExpr::And(..)));
        // Only unquoted operators are recognized.
        assert_eq!(
            parse_err("[[ a '=' b ]]\n").msg,
            "Syntax error: \"'='\" unexpected (expecting \"]]\")"
        );
        // Only as a command name, and not after an alias.
        assert_eq!(simple(&parse("echo [[ a ]]\n")).words.len(), 4);
        for (src, msg) in [
            ("[[ ]]\n", "Syntax error: \"]]\" unexpected"),
            ("[[ -n ]]\n", "Syntax error: \"]]\" unexpected"),
            ("[[ ! ]]\n", "Syntax error: \"]]\" unexpected"),
            ("[[ a b ]]\n", "Syntax error: \"b\" unexpected (expecting \"]]\")"),
            ("[[ a = ]]\n", "Syntax error: \"]]\" unexpected"),
            ("[[ a ]]x\n", "Syntax error: \"]]x\" unexpected (expecting \"]]\")"),
            ("[[ a -a b ]]\n", "Syntax error: \"-a\" unexpected (expecting \"]]\")"),
            ("[[ ( a ]]\n", "Syntax error: \"]]\" unexpected (expecting \")\")"),
            ("[[ a | b ]]\n", "Syntax error: \"|\" unexpected (expecting \"]]\")"),
            ("]]\n", "Syntax error: \"]]\" unexpected"),
        ] {
            assert_eq!(parse_err(src).msg, msg, "{src}");
        }
    }

    #[test]
    fn errors() {
        assert!(parse_err("if true; then\n").msg.contains("end of file"));
        assert_eq!(parse_err("echo ;; x").msg, "Syntax error: \";;\" unexpected");
        assert_eq!(parse_err("a\n\nfi").lineno, 3);
    }

    #[test]
    fn incomplete() {
        for s in [
            "if x",
            "echo 'a",
            "echo \\",
            "cat <<E\nx\n",
            "f() {",
            "a &&",
            "[[ a",
            "[[ a &&\n",
            "[[ a =~ (b",
            "a=(x",
            "a=(x\n",
            "local a=(x",
        ] {
            let mut p = Parser::new(s.as_bytes().to_vec(), 1, false);
            let e = p.parse_next(&Rc::new(AliasMap::default())).unwrap_err();
            assert!(e.incomplete, "{s:?}: {e:?}");
        }
    }

    #[test]
    fn aliases() {
        let mut aliases = AliasMap::default();
        aliases.insert(b"ll".to_vec(), b"ls -l ".to_vec(), false);
        aliases.insert(b"x".to_vec(), b"y".to_vec(), false);
        aliases.insert(b"ls".to_vec(), b"ls -F".to_vec(), false);
        let aliases = Rc::new(aliases);
        let mut p = Parser::new(b"ll x\necho after\n".to_vec(), 1, true);
        let l = p.parse_next(&aliases).unwrap().unwrap();
        let s = simple(&l);
        let words: Vec<_> = s.words.iter().map(|w| w.as_literal().unwrap()).collect();
        assert_eq!(words, vec![&b"ls"[..], b"-F", b"-l", b"y"]);
        let l = p.parse_next(&aliases).unwrap().unwrap();
        assert_eq!(simple(&l).words.len(), 2);
        assert_eq!(p.consumed(), 16);
    }
}
