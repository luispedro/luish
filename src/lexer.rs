//! Tokenizer. The lexer and parser share the [`Parser`] struct because
//! command substitutions (`$(...)`) are parsed recursively in the middle of
//! lexing a word.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::*;

pub type AliasMap = HashMap<Vec<u8>, Vec<u8>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    pub lineno: u32,
    /// The input ended in the middle of a command: more input may fix it.
    pub incomplete: bool,
}

pub type PResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Pipe,
    OrIf,
    Amp,
    AndIf,
    Semi,
    DSemi,
    LParen,
    RParen,
    Less,
    Great,
    DGreat,
    LessAnd,
    GreatAnd,
    LessGreat,
    Clobber,
    DLess,
    DLessDash,
}

impl Op {
    pub fn text(self) -> &'static str {
        match self {
            Op::Pipe => "|",
            Op::OrIf => "||",
            Op::Amp => "&",
            Op::AndIf => "&&",
            Op::Semi => ";",
            Op::DSemi => ";;",
            Op::LParen => "(",
            Op::RParen => ")",
            Op::Less => "<",
            Op::Great => ">",
            Op::DGreat => ">>",
            Op::LessAnd => "<&",
            Op::GreatAnd => ">&",
            Op::LessGreat => "<>",
            Op::Clobber => ">|",
            Op::DLess => "<<",
            Op::DLessDash => "<<-",
        }
    }

    pub fn redir_kind(self) -> Option<RedirKind> {
        Some(match self {
            Op::Less => RedirKind::In,
            Op::Great => RedirKind::Out,
            Op::DGreat => RedirKind::Append,
            Op::LessAnd => RedirKind::DupIn,
            Op::GreatAnd => RedirKind::DupOut,
            Op::LessGreat => RedirKind::ReadWrite,
            Op::Clobber => RedirKind::Clobber,
            Op::DLess | Op::DLessDash => RedirKind::HereDoc,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Word(Word),
    IoNumber(u32),
    Op(Op),
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
    pub lineno: u32,
}

struct PendingHereDoc {
    delim: Vec<u8>,
    strip_tabs: bool,
    quoted: bool,
    body: Rc<RefCell<HereDocBody>>,
}

/// Quoting context for `$...` expansions and nested words.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ctx {
    Unquoted,
    DQuote,
}

pub struct Parser {
    pub(crate) src: Vec<u8>,
    pub(crate) pos: usize,
    pub(crate) lineno: u32,
    /// No more input will arrive after `src`. When false, reaching the end
    /// of `src` is an "incomplete input" error.
    pub(crate) source_eof: bool,
    pub(crate) peeked: Option<Token>,
    pending_heredocs: Vec<PendingHereDoc>,
    pub(crate) aliases: Rc<AliasMap>,
    /// Aliases currently being expanded, with the end of their text in `src`.
    pub(crate) active_aliases: Vec<(Vec<u8>, usize)>,
    /// End of the text of the last alias expanded, if that text ended in a
    /// blank: the word after it is checked for aliases too.
    pub(crate) alias_blank_end: Option<usize>,
    /// Total growth of `src` due to alias substitution.
    pub(crate) splice_delta: isize,
    /// `parse_next` found the start of a command (not just blank lines).
    pub started: bool,
}

pub fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

pub fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

pub fn is_valid_name(s: &[u8]) -> bool {
    !s.is_empty() && is_name_start(s[0]) && s.iter().all(|&c| is_name_char(c))
}

fn is_special_param(c: u8) -> bool {
    matches!(c, b'@' | b'*' | b'#' | b'?' | b'-' | b'$' | b'!' | b'0')
}

impl Parser {
    pub fn new(src: Vec<u8>, lineno: u32, source_eof: bool) -> Parser {
        Parser {
            src,
            pos: 0,
            lineno,
            source_eof,
            peeked: None,
            pending_heredocs: Vec::new(),
            aliases: Rc::new(AliasMap::new()),
            active_aliases: Vec::new(),
            alias_blank_end: None,
            splice_delta: 0,
            started: false,
        }
    }

    /// Bytes of the original input consumed so far (alias substitution
    /// changes the length of `src`, so this corrects for it).
    pub fn consumed(&self) -> usize {
        (self.pos as isize - self.splice_delta) as usize
    }

    /// True if only blanks and newlines are left (comments count as
    /// remaining input, which is conservative).
    pub fn at_end(&self) -> bool {
        self.src[self.pos.min(self.src.len())..]
            .iter()
            .all(|c| matches!(c, b' ' | b'\t' | b'\n'))
    }

    pub(crate) fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError {
            msg: msg.into(),
            lineno: self.lineno,
            incomplete: false,
        })
    }

    fn eof_err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError {
            msg: msg.into(),
            lineno: self.lineno,
            incomplete: !self.source_eof,
        })
    }

    fn incomplete<T>(&self) -> PResult<T> {
        Err(ParseError {
            msg: "Syntax error: end of file unexpected".into(),
            lineno: self.lineno,
            incomplete: true,
        })
    }

    fn at(&self, off: usize) -> Option<u8> {
        self.src.get(self.pos + off).copied()
    }

    /// Removes line continuations at `pos + off`, as dash's `pgetc_eatbnl`
    /// does inside `$` expansions (`$\<newline>?` is `$?`).
    fn eat_bnl(&mut self, off: usize) -> bool {
        let i = self.pos + off;
        let mut ate = false;
        while self.src.get(i) == Some(&b'\\') && self.src.get(i + 1) == Some(&b'\n') {
            self.src.drain(i..i + 2);
            self.splice_delta -= 2;
            self.lineno += 1;
            ate = true;
        }
        ate
    }

    // ------------------------------------------------------------------
    // Tokens

    pub(crate) fn peek(&mut self) -> PResult<&Token> {
        if self.peeked.is_none() {
            let t = self.lex()?;
            self.peeked = Some(t);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    pub(crate) fn next(&mut self) -> PResult<Token> {
        self.peek()?;
        Ok(self.peeked.take().unwrap())
    }

    fn lex(&mut self) -> PResult<Token> {
        loop {
            match self.at(0) {
                Some(b' ' | b'\t') => self.pos += 1,
                Some(b'\\') if self.at(1) == Some(b'\n') => {
                    self.pos += 2;
                    self.lineno += 1;
                }
                Some(b'\\') if self.at(1).is_none() && !self.source_eof => {
                    return self.incomplete();
                }
                Some(b'#') => {
                    while let Some(c) = self.at(0) {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
        let pos = self.pos;
        self.active_aliases.retain(|(_, end)| *end > pos);
        let start = self.pos;
        let lineno = self.lineno;
        let mk = |tok, end| Token {
            tok,
            start,
            end,
            lineno,
        };
        let Some(c) = self.at(0) else {
            if !self.source_eof {
                return self.incomplete();
            }
            if !self.pending_heredocs.is_empty() {
                self.read_heredoc_bodies()?;
            }
            return Ok(mk(Tok::Eof, start));
        };
        let op = |p: &mut Parser, op: Op, len: usize| {
            p.pos += len;
            Ok(mk(Tok::Op(op), p.pos))
        };
        match c {
            b'\n' => {
                self.pos += 1;
                self.lineno += 1;
                let end = self.pos;
                if !self.pending_heredocs.is_empty() {
                    self.read_heredoc_bodies()?;
                }
                Ok(mk(Tok::Newline, end))
            }
            b'|' if self.at(1) == Some(b'|') => op(self, Op::OrIf, 2),
            b'|' => op(self, Op::Pipe, 1),
            b'&' if self.at(1) == Some(b'&') => op(self, Op::AndIf, 2),
            b'&' => op(self, Op::Amp, 1),
            b';' if self.at(1) == Some(b';') => op(self, Op::DSemi, 2),
            b';' => op(self, Op::Semi, 1),
            b'(' => op(self, Op::LParen, 1),
            b')' => op(self, Op::RParen, 1),
            b'<' => match (self.at(1), self.at(2)) {
                (Some(b'<'), Some(b'-')) => op(self, Op::DLessDash, 3),
                (Some(b'<'), _) => op(self, Op::DLess, 2),
                (Some(b'&'), _) => op(self, Op::LessAnd, 2),
                (Some(b'>'), _) => op(self, Op::LessGreat, 2),
                _ => op(self, Op::Less, 1),
            },
            b'>' => match self.at(1) {
                Some(b'>') => op(self, Op::DGreat, 2),
                Some(b'&') => op(self, Op::GreatAnd, 2),
                Some(b'|') => op(self, Op::Clobber, 2),
                _ => op(self, Op::Great, 1),
            },
            _ => {
                let word = self.read_word()?;
                if let Some(lit) = word.as_literal()
                    && lit.iter().all(|c| c.is_ascii_digit())
                    && matches!(self.at(0), Some(b'<' | b'>'))
                    && let Ok(n) = std::str::from_utf8(lit).unwrap().parse::<u32>()
                {
                    return Ok(mk(Tok::IoNumber(n), self.pos));
                }
                Ok(mk(Tok::Word(word), self.pos))
            }
        }
    }

    // ------------------------------------------------------------------
    // Words

    fn read_word(&mut self) -> PResult<Word> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        while let Some(c) = self.at(0) {
            match c {
                b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => break,
                _ => self.read_word_char(c, &mut parts, &mut lit, Ctx::Unquoted)?,
            }
        }
        flush(&mut parts, &mut lit);
        mark_leading_tilde(&mut parts);
        Ok(Word(parts))
    }

    /// Handles one (possibly multi-byte) element of an unquoted word, or of
    /// the word inside `${x-word}`.
    fn read_word_char(&mut self, c: u8, parts: &mut Vec<WordPart>, lit: &mut Vec<u8>, ctx: Ctx) -> PResult<()> {
        match c {
            b'\\' => match self.at(1) {
                None if !self.source_eof => return self.incomplete(),
                None => {
                    lit.push(b'\\');
                    self.pos += 1;
                }
                Some(b'\n') => {
                    self.pos += 2;
                    self.lineno += 1;
                }
                Some(e) => {
                    if ctx == Ctx::DQuote && !matches!(e, b'$' | b'`' | b'"' | b'\\' | b'}') {
                        lit.push(b'\\');
                        self.pos += 1;
                    } else {
                        flush(parts, lit);
                        parts.push(WordPart::Escaped(e));
                        self.pos += 2;
                    }
                }
            },
            b'\'' if ctx == Ctx::Unquoted => {
                flush(parts, lit);
                let start = self.pos + 1;
                let Some(len) = self.src[start..].iter().position(|&c| c == b'\'') else {
                    return self.eof_err("Syntax error: Unterminated quoted string");
                };
                let s = self.src[start..start + len].to_vec();
                self.lineno += s.iter().filter(|&&c| c == b'\n').count() as u32;
                self.pos = start + len + 1;
                parts.push(WordPart::SingleQuoted(s));
            }
            b'"' => {
                flush(parts, lit);
                self.pos += 1;
                let inner = self.read_dquote_parts()?;
                parts.push(WordPart::DoubleQuoted(inner));
            }
            b'$' => match self.read_dollar(ctx)? {
                Some(p) => {
                    flush(parts, lit);
                    parts.push(p);
                }
                None => {
                    lit.push(b'$');
                    self.pos += 1;
                }
            },
            b'`' => {
                flush(parts, lit);
                let p = self.read_backquote(ctx == Ctx::DQuote)?;
                parts.push(p);
            }
            _ => {
                if c == b'\n' {
                    self.lineno += 1;
                }
                lit.push(c);
                self.pos += 1;
            }
        }
        Ok(())
    }

    /// Reads the inside of `"..."`; `pos` is just after the opening quote.
    fn read_dquote_parts(&mut self) -> PResult<Vec<WordPart>> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: Unterminated quoted string");
            };
            match c {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'\\' => match self.at(1) {
                    Some(b'\n') => {
                        self.pos += 2;
                        self.lineno += 1;
                    }
                    Some(e @ (b'$' | b'`' | b'"' | b'\\')) => {
                        flush(&mut parts, &mut lit);
                        parts.push(WordPart::Escaped(e));
                        self.pos += 2;
                    }
                    _ => {
                        lit.push(b'\\');
                        self.pos += 1;
                    }
                },
                b'$' => match self.read_dollar(Ctx::DQuote)? {
                    Some(p) => {
                        flush(&mut parts, &mut lit);
                        parts.push(p);
                    }
                    None => {
                        lit.push(b'$');
                        self.pos += 1;
                    }
                },
                b'`' => {
                    flush(&mut parts, &mut lit);
                    let p = self.read_backquote(true)?;
                    parts.push(p);
                }
                _ => {
                    if c == b'\n' {
                        self.lineno += 1;
                    }
                    lit.push(c);
                    self.pos += 1;
                }
            }
        }
        flush(&mut parts, &mut lit);
        Ok(parts)
    }

    /// At a `$`. Returns `None` if the `$` is literal.
    fn read_dollar(&mut self, ctx: Ctx) -> PResult<Option<WordPart>> {
        let ate = self.eat_bnl(1);
        let Some(c) = self.at(1) else {
            if ate && !self.source_eof {
                return self.incomplete();
            }
            return Ok(None);
        };
        let plain = |name| {
            Some(WordPart::Param(Box::new(ParamExp {
                name,
                op: ParamOp::Plain,
                colon: false,
            })))
        };
        match c {
            b'{' => {
                self.pos += 2;
                self.read_braced_param(ctx).map(Some)
            }
            b'(' if {
                self.eat_bnl(2);
                self.at(2) == Some(b'(')
            } =>
            {
                let save = (self.pos, self.lineno);
                self.pos += 3;
                match self.try_read_arith()? {
                    Some(w) => Ok(Some(WordPart::Arith(w))),
                    None => {
                        (self.pos, self.lineno) = save;
                        self.pos += 2;
                        self.read_cmdsubst().map(Some)
                    }
                }
            }
            b'(' => {
                self.pos += 2;
                self.read_cmdsubst().map(Some)
            }
            c if is_name_start(c) => {
                let start = self.pos + 1;
                let mut end = start;
                while end < self.src.len() && is_name_char(self.src[end]) {
                    end += 1;
                    self.eat_bnl(end - self.pos);
                }
                let name = self.src[start..end].to_vec();
                self.pos = end;
                Ok(plain(ParamName::Var(name)))
            }
            b'1'..=b'9' => {
                self.pos += 2;
                Ok(plain(ParamName::Positional((c - b'0') as usize)))
            }
            c if is_special_param(c) => {
                self.pos += 2;
                Ok(plain(ParamName::Special(c)))
            }
            _ => Ok(None),
        }
    }

    fn read_param_name(&mut self) -> Option<ParamName> {
        self.eat_bnl(0);
        let c = self.at(0)?;
        if is_name_start(c) {
            let start = self.pos;
            while self.at(0).is_some_and(is_name_char) {
                self.pos += 1;
                self.eat_bnl(0);
            }
            Some(ParamName::Var(self.src[start..self.pos].to_vec()))
        } else if c.is_ascii_digit() {
            let start = self.pos;
            while self.at(0).is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
            let n: usize = std::str::from_utf8(&self.src[start..self.pos])
                .unwrap()
                .parse()
                .unwrap_or(usize::MAX);
            Some(if n == 0 {
                ParamName::Special(b'0')
            } else {
                ParamName::Positional(n)
            })
        } else if is_special_param(c) {
            self.pos += 1;
            Some(ParamName::Special(c))
        } else {
            None
        }
    }

    /// After `${`.
    fn read_braced_param(&mut self, ctx: Ctx) -> PResult<WordPart> {
        let bad = |p: &Parser| p.err("Syntax error: Bad substitution");
        let mk = |name, op, colon| WordPart::Param(Box::new(ParamExp { name, op, colon }));
        self.eat_bnl(0);
        if self.at(0) == Some(b'#') {
            // `${#}`, `${#name}` (length), or `${#op...}` ($# with an operator)
            if self.at(1) == Some(b'}') {
                self.pos += 2;
                return Ok(mk(ParamName::Special(b'#'), ParamOp::Plain, false));
            }
            let save = self.pos;
            self.pos += 1;
            if let Some(name) = self.read_param_name()
                && self.at(0) == Some(b'}')
            {
                self.pos += 1;
                return Ok(mk(name, ParamOp::Length, false));
            }
            self.pos = save;
        }
        let Some(name) = self.read_param_name() else {
            return if self.at(0).is_none() {
                self.eof_err("Syntax error: Missing '}'")
            } else {
                bad(self)
            };
        };
        self.eat_bnl(0);
        let Some(c) = self.at(0) else {
            return self.eof_err("Syntax error: Missing '}'");
        };
        if c == b'}' {
            self.pos += 1;
            return Ok(mk(name, ParamOp::Plain, false));
        }
        let colon = c == b':';
        if colon {
            self.pos += 1;
            self.eat_bnl(0);
        }
        let Some(c) = self.at(0) else {
            return self.eof_err("Syntax error: Missing '}'");
        };
        self.pos += 1;
        let mut pattern = false;
        let op: fn(Word) -> ParamOp = match c {
            b'-' => ParamOp::Default,
            b'=' => ParamOp::Assign,
            b'?' => ParamOp::Error,
            b'+' => ParamOp::Alternative,
            b'%' | b'#' if !colon => {
                pattern = true;
                let double = self.at(0) == Some(c);
                if double {
                    self.pos += 1;
                }
                match (c, double) {
                    (b'%', false) => ParamOp::RemoveSmallestSuffix,
                    (b'%', true) => ParamOp::RemoveLargestSuffix,
                    (_, false) => ParamOp::RemoveSmallestPrefix,
                    (_, true) => ParamOp::RemoveLargestPrefix,
                }
            }
            _ => return bad(self),
        };
        // Inside double quotes, the pattern of `%`/`#` starts a fresh quoting
        // context, while the word of the other operators stays quoted.
        let wctx = if pattern { Ctx::Unquoted } else { ctx };
        let word = self.read_param_word(wctx)?;
        Ok(mk(name, op(word), colon))
    }

    /// Reads the word in `${x-word}` up to the closing `}`.
    fn read_param_word(&mut self, ctx: Ctx) -> PResult<Word> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: Missing '}'");
            };
            if c == b'}' {
                self.pos += 1;
                break;
            }
            self.read_word_char(c, &mut parts, &mut lit, ctx)?;
        }
        flush(&mut parts, &mut lit);
        if ctx == Ctx::Unquoted {
            mark_leading_tilde(&mut parts);
        }
        Ok(Word(parts))
    }

    /// After `$((`. Returns `None` if this turns out to be `$( (...) )`.
    fn try_read_arith(&mut self) -> PResult<Option<Word>> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        let mut depth = 0usize;
        loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: Missing '))'");
            };
            match c {
                b'(' => {
                    depth += 1;
                    lit.push(c);
                    self.pos += 1;
                }
                b')' if depth > 0 => {
                    depth -= 1;
                    lit.push(c);
                    self.pos += 1;
                }
                b')' => {
                    if self.at(1) == Some(b')') {
                        self.pos += 2;
                        break;
                    }
                    return Ok(None);
                }
                b'\'' => {
                    lit.push(c);
                    self.pos += 1;
                }
                _ => self.read_word_char(c, &mut parts, &mut lit, Ctx::DQuote)?,
            }
        }
        flush(&mut parts, &mut lit);
        Ok(Some(Word(parts)))
    }

    /// After `$(`: parse a nested command list up to the matching `)`.
    fn read_cmdsubst(&mut self) -> PResult<WordPart> {
        debug_assert!(self.peeked.is_none());
        let outer_heredocs = std::mem::take(&mut self.pending_heredocs);
        let outer_alias_blank_end = self.alias_blank_end.take();
        let list = self.parse_compound_list()?;
        let t = self.next()?;
        if t.tok != Tok::Op(Op::RParen) {
            return self.unexpected(&t, Some(")"));
        }
        self.pending_heredocs = outer_heredocs;
        self.alias_blank_end = outer_alias_blank_end;
        Ok(WordPart::CmdSubst(Rc::new(list)))
    }

    /// At a backquote: collect the text up to the closing backquote,
    /// unescape it, and parse it separately.
    fn read_backquote(&mut self, in_dquote: bool) -> PResult<WordPart> {
        self.pos += 1;
        let lineno = self.lineno;
        let mut text = Vec::new();
        loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: EOF in backquote substitution");
            };
            self.pos += 1;
            match c {
                b'`' => break,
                b'\\' => match self.at(0) {
                    Some(e @ (b'$' | b'`' | b'\\')) => {
                        text.push(e);
                        self.pos += 1;
                    }
                    Some(b'"') if in_dquote => {
                        text.push(b'"');
                        self.pos += 1;
                    }
                    _ => text.push(b'\\'),
                },
                _ => {
                    if c == b'\n' {
                        self.lineno += 1;
                    }
                    text.push(c);
                }
            }
        }
        let mut sub = Parser::new(text, lineno, true);
        sub.aliases = self.aliases.clone();
        let list = sub.parse_all()?;
        Ok(WordPart::CmdSubst(Rc::new(list)))
    }

    // ------------------------------------------------------------------
    // Here-documents

    pub(crate) fn push_heredoc(&mut self, raw: &[u8], strip_tabs: bool) -> Rc<RefCell<HereDocBody>> {
        let (delim, quoted) = unquote_delim(raw);
        let body = Rc::new(RefCell::new(HereDocBody::default()));
        self.pending_heredocs.push(PendingHereDoc {
            delim,
            strip_tabs,
            quoted,
            body: body.clone(),
        });
        body
    }

    fn read_heredoc_bodies(&mut self) -> PResult<()> {
        for hd in std::mem::take(&mut self.pending_heredocs) {
            let lineno = self.lineno;
            let mut body = Vec::new();
            loop {
                if self.pos >= self.src.len() {
                    if !self.source_eof {
                        return self.incomplete();
                    }
                    break;
                }
                let rest = &self.src[self.pos..];
                let (line, advance) = match rest.iter().position(|&c| c == b'\n') {
                    Some(i) => (&rest[..i], i + 1),
                    None if !self.source_eof => return self.incomplete(),
                    None => (rest, rest.len()),
                };
                let mut line = line;
                if hd.strip_tabs {
                    while let [b'\t', tail @ ..] = line {
                        line = tail;
                    }
                }
                let done = line == hd.delim.as_slice();
                if !done {
                    body.extend_from_slice(line);
                    body.push(b'\n');
                }
                self.pos += advance;
                self.lineno += 1;
                if done {
                    break;
                }
            }
            let word = if hd.quoted {
                Word(vec![WordPart::Literal(body)])
            } else {
                let mut sub = Parser::new(body, lineno, true);
                sub.aliases = self.aliases.clone();
                sub.read_heredoc_word()?
            };
            *hd.body.borrow_mut() = HereDocBody {
                body: word,
                quoted: hd.quoted,
            };
        }
        Ok(())
    }

    fn read_heredoc_word(&mut self) -> PResult<Word> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        while let Some(c) = self.at(0) {
            match c {
                b'\\' => match self.at(1) {
                    Some(b'\n') => {
                        self.pos += 2;
                        self.lineno += 1;
                    }
                    Some(e @ (b'$' | b'`' | b'\\')) => {
                        flush(&mut parts, &mut lit);
                        parts.push(WordPart::Escaped(e));
                        self.pos += 2;
                    }
                    _ => {
                        lit.push(b'\\');
                        self.pos += 1;
                    }
                },
                b'$' => match self.read_dollar(Ctx::DQuote)? {
                    Some(p) => {
                        flush(&mut parts, &mut lit);
                        parts.push(p);
                    }
                    None => {
                        lit.push(b'$');
                        self.pos += 1;
                    }
                },
                b'`' => {
                    flush(&mut parts, &mut lit);
                    let p = self.read_backquote(false)?;
                    parts.push(p);
                }
                _ => {
                    if c == b'\n' {
                        self.lineno += 1;
                    }
                    lit.push(c);
                    self.pos += 1;
                }
            }
        }
        flush(&mut parts, &mut lit);
        // Everything in a here-doc body is quoted, as if in double quotes.
        Ok(Word(vec![WordPart::DoubleQuoted(parts)]))
    }

    // ------------------------------------------------------------------
    // Aliases

    /// If the next token is a word naming an alias that is not already being
    /// expanded, substitute the alias text into the input.
    pub(crate) fn maybe_expand_alias(&mut self) -> PResult<()> {
        if self.aliases.is_empty() {
            return Ok(());
        }
        loop {
            let name = match &self.peek()?.tok {
                Tok::Word(w) => match w.as_literal() {
                    Some(name) => name.to_vec(),
                    None => return Ok(()),
                },
                _ => return Ok(()),
            };
            if crate::parser::is_reserved(&name) || self.active_aliases.iter().any(|(n, _)| *n == name) {
                return Ok(());
            }
            let Some(value) = self.aliases.get(&name).cloned() else {
                return Ok(());
            };
            let tok = self.peeked.take().unwrap();
            let delta = value.len() as isize - (tok.end - tok.start) as isize;
            for end in self
                .active_aliases
                .iter_mut()
                .map(|(_, end)| end)
                .chain(self.alias_blank_end.as_mut())
            {
                if *end >= tok.end {
                    *end = (*end as isize + delta) as usize;
                }
            }
            self.src.splice(tok.start..tok.end, value.iter().copied());
            self.splice_delta += delta;
            self.pos = tok.start;
            self.lineno = tok.lineno;
            self.active_aliases.push((name, tok.start + value.len()));
            if value.last().is_some_and(|&c| c == b' ' || c == b'\t') {
                self.alias_blank_end = Some(tok.start + value.len());
            }
        }
    }

    /// Source text of a token, for error messages.
    pub(crate) fn token_text(&self, t: &Token) -> String {
        match &t.tok {
            Tok::Eof => "end of file".into(),
            Tok::Newline => "newline".into(),
            Tok::Op(op) => format!("\"{}\"", op.text()),
            _ => format!(
                "\"{}\"",
                String::from_utf8_lossy(&self.src[t.start..t.end.min(self.src.len())])
            ),
        }
    }
}

fn flush(parts: &mut Vec<WordPart>, lit: &mut Vec<u8>) {
    if !lit.is_empty() {
        parts.push(WordPart::Literal(std::mem::take(lit)));
    }
}

/// Converts a leading unquoted `~prefix` into a [`WordPart::Tilde`].
fn mark_leading_tilde(parts: &mut Vec<WordPart>) {
    let Some(WordPart::Literal(s)) = parts.first() else {
        return;
    };
    if s.first() != Some(&b'~') {
        return;
    }
    let end = s.iter().position(|&c| c == b'/');
    if end.is_none() && parts.len() > 1 {
        return; // part of the prefix is quoted or an expansion
    }
    let end = end.unwrap_or(s.len());
    let user = s[1..end].to_vec();
    let rest = s[end..].to_vec();
    parts[0] = WordPart::Tilde(user);
    if !rest.is_empty() {
        parts.insert(1, WordPart::Literal(rest));
    }
}

/// Tilde prefixes in an assignment value: at the start and after each `:`.
pub(crate) fn mark_assignment_tildes(parts: Vec<WordPart>) -> Vec<WordPart> {
    let n = parts.len();
    let mut out = Vec::with_capacity(n);
    let mut at_start = true;
    for (i, part) in parts.into_iter().enumerate() {
        let WordPart::Literal(s) = part else {
            at_start = false;
            out.push(mark_param_word_tildes(part));
            continue;
        };
        let last = i + 1 == n;
        let mut lit = Vec::new();
        let mut j = 0;
        while j < s.len() {
            let tilde_ok = s[j] == b'~' && (if j == 0 { at_start } else { s[j - 1] == b':' });
            if tilde_ok {
                let end = s[j..].iter().position(|&c| c == b'/' || c == b':').map(|k| j + k);
                if end.is_some() || last {
                    let end = end.unwrap_or(s.len());
                    if !lit.is_empty() {
                        out.push(WordPart::Literal(std::mem::take(&mut lit)));
                    }
                    out.push(WordPart::Tilde(s[j + 1..end].to_vec()));
                    j = end;
                    continue;
                }
            }
            lit.push(s[j]);
            j += 1;
        }
        if !lit.is_empty() {
            out.push(WordPart::Literal(lit));
        }
        at_start = false;
    }
    out
}

/// In an assignment, the word of an unquoted `${x-word}` (and `+`, `=`,
/// `?`) also gets tilde expansion after `:` (as in dash).
fn mark_param_word_tildes(part: WordPart) -> WordPart {
    let WordPart::Param(mut pe) = part else {
        return part;
    };
    if let ParamOp::Default(w) | ParamOp::Assign(w) | ParamOp::Error(w) | ParamOp::Alternative(w) = &mut pe.op {
        let mut parts = std::mem::take(&mut w.0);
        // Undo the leading tilde prefix, which ended only at `/`.
        if let Some(WordPart::Tilde(user)) = parts.first() {
            let mut lit = vec![b'~'];
            lit.extend_from_slice(user);
            if let Some(WordPart::Literal(rest)) = parts.get(1) {
                lit.extend_from_slice(rest);
                parts.remove(1);
            }
            parts[0] = WordPart::Literal(lit);
        }
        w.0 = mark_assignment_tildes(parts);
    }
    WordPart::Param(pe)
}

/// Quote removal on a here-doc delimiter; also reports whether any part of
/// it was quoted.
fn unquote_delim(raw: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'\'' => {
                quoted = true;
                i += 1;
                while i < raw.len() && raw[i] != b'\'' {
                    out.push(raw[i]);
                    i += 1;
                }
                i += 1;
            }
            b'"' => {
                quoted = true;
                i += 1;
                while i < raw.len() && raw[i] != b'"' {
                    if raw[i] == b'\\' && i + 1 < raw.len() && b"$`\"\\".contains(&raw[i + 1]) {
                        i += 1;
                    }
                    out.push(raw[i]);
                    i += 1;
                }
                i += 1;
            }
            b'\\' => {
                quoted = true;
                if i + 1 < raw.len() {
                    out.push(raw[i + 1]);
                }
                i += 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, quoted)
}

/// Parses text as if it were the body of an unquoted here-document: `$`
/// expansions, backquotes, and backslash escapes are recognized (for
/// prompts).
pub fn parse_string_word(text: &[u8]) -> PResult<Word> {
    let mut p = Parser::new(text.to_vec(), 1, true);
    p.read_heredoc_word()
}
