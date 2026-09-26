//! Recursive-descent parser for the POSIX shell grammar (XCU §2.10).

use std::rc::Rc;

use crate::ast::*;
use crate::lexer::{AliasMap, Op, PResult, ParseError, Parser, Tok, Token, is_valid_name};

const RESERVED: &[&[u8]] = &[
    b"!", b"{", b"}", b"case", b"do", b"done", b"elif", b"else", b"esac", b"fi", b"for", b"if", b"in", b"then",
    b"until", b"while",
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
        self.maybe_expand_alias()?;
        if self.peek_op()? == Some(Op::LParen) {
            let cmd = self.parse_compound()?;
            let redirs = self.parse_redirects()?;
            return Ok(Command::Compound(cmd, redirs));
        }
        if let Some(w) = self.peek_literal()? {
            match w.as_slice() {
                b"{" | b"if" | b"while" | b"until" | b"for" | b"case" => {
                    let cmd = self.parse_compound()?;
                    let redirs = self.parse_redirects()?;
                    return Ok(Command::Compound(cmd, redirs));
                }
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
            _ => self.unexpected(&t, None),
        }
    }

    fn parse_for(&mut self, lineno: u32) -> PResult<CompoundCommand> {
        let t = self.next()?;
        let var = match &t.tok {
            Tok::Word(w) if w.as_literal().is_some_and(is_valid_name) => w.as_literal().unwrap().to_vec(),
            Tok::Word(_) => return self.err("Syntax error: Bad for loop variable"),
            _ => return self.unexpected(&t, None),
        };
        let mut words = None;
        self.skip_newlines()?;
        if self.peek_is_kw(b"in")? {
            self.next()?;
            let mut ws = Vec::new();
            loop {
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
        let t = self.next()?;
        let Tok::Word(word) = t.tok else {
            return self.unexpected(&t, None);
        };
        self.skip_newlines()?;
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
        let t = self.next()?;
        let Tok::Word(word) = t.tok else {
            return self.unexpected(&t, None);
        };
        let target = if kind == RedirKind::HereDoc {
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
            } else if let Some(end) = self.alias_blank_end
                && self.peek()?.start >= end
            {
                self.alias_blank_end = None;
                self.maybe_expand_alias()?;
            }
            if !matches!(self.peek()?.tok, Tok::Word(_)) {
                break;
            }
            let Tok::Word(w) = self.next()?.tok else { unreachable!() };
            if words.is_empty()
                && let Some(a) = split_assignment(&w)
            {
                assigns.push(a);
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
        let is_compound = match &self.peek()?.tok {
            Tok::Op(Op::LParen) => true,
            Tok::Word(w) => matches!(
                w.as_literal(),
                Some(b"{" | b"if" | b"while" | b"until" | b"for" | b"case")
            ),
            _ => false,
        };
        if !is_compound {
            let t = self.next()?;
            return self.unexpected(&t, None);
        }
        let cmd = self.parse_compound()?;
        let redirs = self.parse_redirects()?;
        Ok(Command::FunctionDef {
            name,
            body: Rc::new(FunctionBody { cmd, redirs }),
        })
    }
}

/// Splits `NAME=value` into an assignment, if the word has that form.
pub(crate) fn split_assignment(w: &Word) -> Option<Assign> {
    let Some(WordPart::Literal(s)) = w.0.first() else {
        return None;
    };
    let eq = s.iter().position(|&c| c == b'=')?;
    if !is_valid_name(&s[..eq]) {
        return None;
    }
    let mut parts = Vec::new();
    if eq + 1 < s.len() {
        parts.push(WordPart::Literal(s[eq + 1..].to_vec()));
    }
    parts.extend(w.0[1..].iter().cloned());
    Some(Assign {
        name: s[..eq].to_vec(),
        value: Word(crate::lexer::mark_assignment_tildes(parts)),
    })
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
    fn errors() {
        assert!(parse_err("if true; then\n").msg.contains("end of file"));
        assert_eq!(parse_err("echo ;; x").msg, "Syntax error: \";;\" unexpected");
        assert_eq!(parse_err("a\n\nfi").lineno, 3);
    }

    #[test]
    fn incomplete() {
        for s in ["if x", "echo 'a", "echo \\", "cat <<E\nx\n", "f() {", "a &&"] {
            let mut p = Parser::new(s.as_bytes().to_vec(), 1, false);
            let e = p.parse_next(&Rc::new(AliasMap::new())).unwrap_err();
            assert!(e.incomplete, "{s:?}: {e:?}");
        }
    }

    #[test]
    fn aliases() {
        let mut aliases = AliasMap::new();
        aliases.insert(b"ll".to_vec(), b"ls -l ".to_vec());
        aliases.insert(b"x".to_vec(), b"y".to_vec());
        aliases.insert(b"ls".to_vec(), b"ls -F".to_vec());
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
