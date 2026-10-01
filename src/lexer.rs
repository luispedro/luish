//! Tokenizer. The lexer and parser share the [`Parser`] struct because
//! command substitutions (`$(...)`) are parsed recursively in the middle of
//! lexing a word.

use crate::hash::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::*;

/// The aliases. Regular and global aliases share one table (as in zsh, a
/// name is one or the other); suffix aliases, keyed by what follows the
/// last `.` of a command name, have their own.
#[derive(Clone, Default)]
pub struct AliasMap {
    names: HashMap<Vec<u8>, Alias>,
    suffixes: HashMap<Vec<u8>, Vec<u8>>,
    /// The number of global aliases: without any, words that aren't
    /// command names aren't looked up.
    globals: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub value: Vec<u8>,
    /// Expanded in any position, not only as a command name (`alias -g`).
    pub global: bool,
}

/// The kinds of alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasKind {
    Regular,
    Global,
    Suffix,
}

impl Alias {
    /// Regular or global.
    pub fn kind(&self) -> AliasKind {
        if self.global {
            AliasKind::Global
        } else {
            AliasKind::Regular
        }
    }
}

impl AliasMap {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.suffixes.is_empty()
    }

    pub fn has_globals(&self) -> bool {
        self.globals > 0
    }

    /// The regular or global alias `name`.
    pub fn get(&self, name: &[u8]) -> Option<&Alias> {
        self.names.get(name)
    }

    pub fn contains(&self, name: &[u8]) -> bool {
        self.names.contains_key(name)
    }

    pub fn insert(&mut self, name: Vec<u8>, value: Vec<u8>, global: bool) {
        let old = self.names.insert(name, Alias { value, global });
        self.globals = self.globals + usize::from(global) - usize::from(old.is_some_and(|a| a.global));
    }

    pub fn remove(&mut self, name: &[u8]) -> bool {
        let old = self.names.remove(name);
        self.globals -= usize::from(old.as_ref().is_some_and(|a| a.global));
        old.is_some()
    }

    /// Removes the regular and global aliases.
    pub fn clear(&mut self) {
        self.names.clear();
        self.globals = 0;
    }

    /// The regular and global aliases, by name.
    pub fn sorted(&self) -> Vec<(&[u8], &Alias)> {
        let mut v: Vec<_> = self.names.iter().map(|(n, a)| (&n[..], a)).collect();
        v.sort_unstable_by_key(|e| e.0);
        v
    }

    pub fn insert_suffix(&mut self, suffix: Vec<u8>, value: Vec<u8>) {
        self.suffixes.insert(suffix, value);
    }

    pub fn remove_suffix(&mut self, suffix: &[u8]) -> bool {
        self.suffixes.remove(suffix).is_some()
    }

    pub fn clear_suffixes(&mut self) {
        self.suffixes.clear();
    }

    /// The suffix aliases, by suffix.
    pub fn sorted_suffixes(&self) -> Vec<(&[u8], &[u8])> {
        let mut v: Vec<_> = self.suffixes.iter().map(|(s, v)| (&s[..], &v[..])).collect();
        v.sort_unstable_by_key(|e| e.0);
        v
    }

    /// The suffix alias (suffix and value) that applies to the command name
    /// `name`: one for what follows its last `.`, if neither that nor what
    /// precedes it is empty.
    pub fn for_suffix(&self, name: &[u8]) -> Option<(&[u8], &[u8])> {
        if self.suffixes.is_empty() {
            return None;
        }
        let dot = name
            .iter()
            .rposition(|&c| c == b'.')
            .filter(|&i| i > 0 && i + 1 < name.len())?;
        let (s, v) = self.suffixes.get_key_value(&name[dot + 1..])?;
        Some((s, v))
    }

    /// Whether the word `w` would be expanded as an alias, as a command
    /// name if `command` (for quoting a word that must stay as it is).
    pub fn expands(&self, w: &[u8], command: bool) -> bool {
        match self.get(w) {
            Some(a) => command || a.global,
            None => command && self.for_suffix(w).is_some(),
        }
    }
}

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
    /// A suffix alias is recorded as its suffix after a NUL byte, which
    /// can't be in the name of another alias.
    pub(crate) active_aliases: Vec<(Vec<u8>, usize)>,
    /// The next word is a here-document delimiter, which isn't expanded
    /// as a global alias.
    raw_word: bool,
    /// The next word is the right side of `=~` in `[[ ... ]]`, where `(`
    /// and `|` are part of the word (`read_word`).
    pub(crate) regex_word: bool,
    /// End of the text of the last alias expanded, if that text ended in a
    /// blank: the word after it is checked for aliases too.
    pub(crate) alias_blank_end: Option<usize>,
    /// Total growth of `src` due to alias substitution.
    pub(crate) splice_delta: isize,
    /// `parse_next` found the start of a command (not just blank lines).
    pub started: bool,
    /// `setopt glob.bare_qualifiers`: a trailing `(...)` in a word is a glob
    /// qualifier. This is the only place the lexer depends on an option.
    pub bareglobqual: bool,
}

/// Whether the word after an alias with this value is checked for aliases.
pub fn ends_in_blank(value: &[u8]) -> bool {
    value.last().is_some_and(|&c| c == b' ' || c == b'\t')
}

pub fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

pub fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Whether a word so far is `name=` or `name+=`, so that a `(` after it
/// starts an array rather than a glob qualifier.
fn is_array_start(lit: &[u8]) -> bool {
    let Some(name) = lit.strip_suffix(b"=") else {
        return false;
    };
    is_valid_name(name.strip_suffix(b"+").unwrap_or(name))
}

pub fn is_valid_name(s: &[u8]) -> bool {
    !s.is_empty() && is_name_start(s[0]) && s.iter().all(|&c| is_name_char(c))
}

fn is_special_param(c: u8) -> bool {
    matches!(c, b'@' | b'*' | b'#' | b'?' | b'-' | b'$' | b'!' | b'0')
}

thread_local! {
    /// An empty alias table, shared so that a new parser (as for each
    /// `eval`) doesn't allocate one.
    static NO_ALIASES: Rc<AliasMap> = Rc::new(AliasMap::default());
}

/// The shared empty alias table.
pub fn no_aliases() -> Rc<AliasMap> {
    NO_ALIASES.with(Rc::clone)
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
            aliases: no_aliases(),
            active_aliases: Vec::new(),
            alias_blank_end: None,
            raw_word: false,
            regex_word: false,
            splice_delta: 0,
            started: false,
            bareglobqual: false,
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

    #[inline]
    pub(crate) fn peek(&mut self) -> PResult<&Token> {
        if self.peeked.is_none() {
            self.lex_next()?;
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    /// Reads the next token into `peeked`.
    fn lex_next(&mut self) -> PResult<()> {
        let t = self.lex()?;
        self.peeked = Some(t);
        if self.aliases.has_globals() && !self.raw_word {
            self.expand_global()?;
        }
        Ok(())
    }

    /// Reads the next token, a here-document delimiter, as it is written.
    pub(crate) fn next_raw(&mut self) -> PResult<Token> {
        self.raw_word = true;
        let t = self.next();
        self.raw_word = false;
        t
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
            b'(' | b'|' if self.regex_word => self.lex_word(start, lineno),
            b'|' if self.at(1) == Some(b'|') => op(self, Op::OrIf, 2),
            b'|' => op(self, Op::Pipe, 1),
            b'&' if self.at(1) == Some(b'&') => op(self, Op::AndIf, 2),
            b'&' => op(self, Op::Amp, 1),
            b';' if self.at(1) == Some(b';') => op(self, Op::DSemi, 2),
            b';' => op(self, Op::Semi, 1),
            b'(' => op(self, Op::LParen, 1),
            b')' => op(self, Op::RParen, 1),
            // A word that starts with `<(` or `>(`; `read_word` reads it.
            b'<' | b'>' if self.at(1) == Some(b'(') && !self.regex_word => self.lex_word(start, lineno),
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
            _ => self.lex_word(start, lineno),
        }
    }

    fn lex_word(&mut self, start: usize, lineno: u32) -> PResult<Token> {
        let word = self.read_word()?;
        let tok = match word.as_literal() {
            Some(lit)
                if lit.iter().all(|c| c.is_ascii_digit())
                    && matches!(self.at(0), Some(b'<' | b'>'))
                    && let Ok(n) = std::str::from_utf8(lit).unwrap().parse::<u32>() =>
            {
                Tok::IoNumber(n)
            }
            _ => Tok::Word(word),
        };
        Ok(Token {
            tok,
            start,
            end: self.pos,
            lineno,
        })
    }

    // ------------------------------------------------------------------
    // Words

    fn read_word(&mut self) -> PResult<Word> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        let mut qual = None;
        // Unclosed `(` in a regular expression (`regex_word`).
        let mut depth = 0;
        while let Some(c) = self.at(0) {
            match c {
                b'(' if self.bareglobqual
                    && !self.regex_word
                    && !(parts.is_empty() && (lit.is_empty() || is_array_start(&lit))) =>
                {
                    qual = self.read_glob_qualifier();
                    break;
                }
                // Process substitution, also inside a word (`--input=<(cmd)`), as
                // in bash and zsh. It is an error in POSIX sh.
                b'<' | b'>' if !self.regex_word && self.at(1) == Some(b'(') => {
                    flush(&mut parts, &mut lit);
                    self.pos += 2;
                    let list = self.read_subst_list()?;
                    parts.push(WordPart::ProcSubst {
                        output: c == b'>',
                        list,
                    });
                }
                b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b'(' | b')' => {
                    if !self.regex_word {
                        break;
                    }
                    match c {
                        b'(' => depth += 1,
                        b')' if depth > 0 => depth -= 1,
                        b'|' => {}
                        _ if depth == 0 => break,
                        b'\n' => self.lineno += 1,
                        _ => {}
                    }
                    lit.push(c);
                    self.pos += 1;
                }
                _ => self.read_word_char(c, &mut parts, &mut lit, Ctx::Unquoted)?,
            }
        }
        flush(&mut parts, &mut lit);
        mark_leading_tilde(&mut parts);
        parts.extend(qual.map(WordPart::GlobQual));
        Ok(Word(parts))
    }

    /// At a `(` inside a word: reads a glob qualifier, `(...)` at the end of
    /// the word with no blanks, quotes or operators inside. Anything else,
    /// including the `()` of a function definition, is left alone.
    fn read_glob_qualifier(&mut self) -> Option<Vec<u8>> {
        let rest = &self.src[self.pos + 1..];
        let len = rest.iter().position(|c| b"() \t\n;&|<>'\"\\$`".contains(c))?;
        let ends_word = |c: Option<&u8>| {
            c.is_none_or(|c| matches!(c, b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'<' | b'>' | b')'))
        };
        if len == 0 || rest[len] != b')' || !ends_word(rest.get(len + 1)) {
            return None;
        }
        let q = rest[..len].to_vec();
        self.pos += len + 2;
        Some(q)
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
        // Nested `$(`, `${` and `$((` recurse through here, `$(` without
        // reaching parse_command.
        if !crate::stack::ok() {
            return self.err(crate::stack::TOO_DEEP);
        }
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
                index: None,
                op: ParamOp::Plain,
                colon: false,
                flags: None,
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
        // As in dash, a bad substitution is an error only when expanded:
        // the rest up to `}` is read as its word.
        let bad = |p: &mut Parser, name, colon| {
            let word = p.read_param_word(ctx)?;
            Ok(WordPart::Param(Box::new(ParamExp {
                name,
                index: None,
                op: ParamOp::Bad(word),
                colon,
                flags: None,
            })))
        };
        let mk = |name, op, colon| {
            WordPart::Param(Box::new(ParamExp {
                name,
                index: None,
                op,
                colon,
                flags: None,
            }))
        };
        self.eat_bnl(0);
        if self.at(0) == Some(b'(') {
            // zsh's `${(flags)name...}`, a bad substitution in dash. The
            // rest is read as usual, but without `${#name}` and `${!name}`.
            let start = (self.pos, self.lineno);
            let part = match self.read_flags()? {
                Some(flags) => self.read_flagged_param(flags, ctx)?,
                None => None,
            };
            return match part {
                Some(part) => Ok(part),
                None => {
                    // Read as a whole, from the `(`.
                    (self.pos, self.lineno) = start;
                    bad(self, ParamName::Var(Vec::new()), false)
                }
            };
        }
        if self.at(0) == Some(b'#') {
            // `${#}`, `${#name}` (length), or `${#op...}` ($# with an operator)
            if self.at(1) == Some(b'}') {
                self.pos += 2;
                return Ok(mk(ParamName::Special(b'#'), ParamOp::Plain, false));
            }
            let save = self.pos;
            self.pos += 1;
            if let Some(name) = self.read_param_name() {
                let index = match self.at(0) {
                    Some(b'[') if matches!(name, ParamName::Var(_)) => self.read_index()?,
                    _ => None,
                };
                if self.at(0) == Some(b'}') {
                    self.pos += 1;
                    return Ok(WordPart::Param(Box::new(ParamExp {
                        name,
                        index,
                        op: ParamOp::Length,
                        colon: false,
                        flags: None,
                    })));
                }
            }
            self.pos = save;
        }
        if self.at(0) == Some(b'!') && self.at(1).is_some_and(|c| is_name_start(c) || c.is_ascii_digit()) {
            // bash's `${!a[@]}` and `${!a[*]}` (keys), `${!prefix@}` and
            // `${!prefix*}` (names), and `${!name...}` (indirection, also
            // with an operator). In dash, these are bad substitutions.
            self.pos += 1;
            let save = (self.pos, self.lineno);
            let Some(name) = self.read_param_name() else {
                unreachable!()
            };
            let list = |name, index, op| {
                Ok(WordPart::Param(Box::new(ParamExp {
                    name,
                    index: Some(index),
                    op,
                    colon: false,
                    flags: None,
                })))
            };
            if matches!(name, ParamName::Var(_)) {
                match (self.at(0), self.at(1)) {
                    (Some(c @ (b'@' | b'*')), Some(b'}')) => {
                        self.pos += 2;
                        let index = if c == b'@' { Index::At } else { Index::Star };
                        return list(name, index, ParamOp::Names);
                    }
                    (Some(b'['), _) => match self.read_index()? {
                        Some(index @ (Index::At | Index::Star)) if self.at(0) == Some(b'}') => {
                            self.pos += 1;
                            return list(name, index, ParamOp::Keys);
                        }
                        Some(index @ Index::Expr(_)) => {
                            let mut part = self.read_param_op(ParamName::Indirect(Box::new(name)), ctx)?;
                            if let WordPart::Param(pe) = &mut part {
                                pe.index = Some(index);
                            }
                            return Ok(part);
                        }
                        _ => {
                            (self.pos, self.lineno) = save;
                            return bad(self, ParamName::Special(b'!'), false);
                        }
                    },
                    _ => {}
                }
            }
            return self.read_param_op(ParamName::Indirect(Box::new(name)), ctx);
        }
        let Some(name) = self.read_param_name() else {
            return if self.at(0).is_none() {
                self.eof_err("Syntax error: Missing '}'")
            } else {
                bad(self, ParamName::Var(Vec::new()), false)
            };
        };
        if matches!(name, ParamName::Var(_)) && self.at(0) == Some(b'[') {
            let save = self.pos;
            match self.read_index()? {
                Some(index) => {
                    let mut part = self.read_param_op(name, ctx)?;
                    if let WordPart::Param(pe) = &mut part {
                        pe.index = Some(index);
                    }
                    return Ok(part);
                }
                None => {
                    self.pos = save;
                    return bad(self, name, false);
                }
            }
        }
        self.read_param_op(name, ctx)
    }

    /// Reads zsh's `(flags)` of `${(flags)name...}`, from the `(` to the
    /// `)`. Returns `None` for a flag luish doesn't have.
    fn read_flags(&mut self) -> PResult<Option<Flags>> {
        let start = self.pos + 1;
        self.pos = start;
        let mut fl = Flags::default();
        loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: Missing '}'");
            };
            self.pos += 1;
            match c {
                b')' => break,
                b'@' => fl.at = true,
                b'k' => fl.keys = true,
                b'v' => fl.values = true,
                b'j' | b's' => {
                    // The separator is between two delimiters, which can
                    // also be a pair of brackets: `j:,:`, `s(,)`. As in
                    // zsh, it can't contain `}`, which ends the expansion.
                    let Some(open) = self.at(0) else {
                        return self.eof_err("Syntax error: Missing '}'");
                    };
                    let close = match open {
                        b'(' => b')',
                        b'[' => b']',
                        b'<' => b'>',
                        c => c,
                    };
                    self.pos += 1;
                    let from = self.pos;
                    loop {
                        match self.at(0) {
                            None => return self.eof_err("Syntax error: Missing '}'"),
                            Some(b'}') => return Ok(None),
                            Some(c) if c == close => break,
                            Some(_) => self.pos += 1,
                        }
                    }
                    let sep = self.src[from..self.pos].to_vec();
                    self.pos += 1;
                    if c == b'j' {
                        fl.join = Some(sep);
                    } else {
                        fl.split = Some(sep);
                    }
                }
                b'F' => fl.join = Some(b"\n".to_vec()),
                b'f' => fl.split = Some(b"\n".to_vec()),
                b'L' => fl.case = Some(Case::Lower),
                b'U' => fl.case = Some(Case::Upper),
                b'C' => fl.case = Some(Case::Capitalize),
                b'u' => fl.unique = true,
                b'o' => fl.sort = true,
                b'O' => fl.reverse = true,
                b'i' => fl.nocase = true,
                b'n' => fl.numeric = true,
                b'a' => fl.array_order = true,
                _ => return Ok(None),
            }
        }
        fl.text = self.src[start..self.pos - 1].to_vec();
        Ok(Some(fl))
    }

    /// The rest of `${(flags)name...}`, after the flags. Returns `None` if
    /// there is no name, or no `]` after `[`.
    fn read_flagged_param(&mut self, flags: Flags, ctx: Ctx) -> PResult<Option<WordPart>> {
        let Some(name) = self.read_param_name() else {
            return Ok(None);
        };
        let index = match self.at(0) {
            Some(b'[') if matches!(name, ParamName::Var(_)) => match self.read_index()? {
                Some(index) => Some(index),
                None => return Ok(None),
            },
            _ => None,
        };
        let mut part = self.read_param_op(name, ctx)?;
        if let WordPart::Param(pe) = &mut part {
            pe.index = index;
            pe.flags = Some(Box::new(flags));
        }
        Ok(Some(part))
    }

    /// Reads `[index]` of `${name[index]}`, from the `[`. Returns `None`,
    /// leaving the position anywhere, if there is no `]` before the `}`.
    fn read_index(&mut self) -> PResult<Option<Index>> {
        self.pos += 1;
        match (self.at(0), self.at(1)) {
            (Some(b'@'), Some(b']')) => {
                self.pos += 2;
                return Ok(Some(Index::At));
            }
            (Some(b'*'), Some(b']')) => {
                self.pos += 2;
                return Ok(Some(Index::Star));
            }
            _ => {}
        }
        let (w, closed) = self.read_param_word_to(Ctx::DQuote, Some(b']'))?;
        Ok(closed.then_some(Index::Expr(w)))
    }

    /// The rest of `${name...}`, after the name (and the index).
    fn read_param_op(&mut self, name: ParamName, ctx: Ctx) -> PResult<WordPart> {
        let bad = |p: &mut Parser, name, colon| {
            let word = p.read_param_word(ctx)?;
            Ok(WordPart::Param(Box::new(ParamExp {
                name,
                index: None,
                op: ParamOp::Bad(word),
                colon,
                flags: None,
            })))
        };
        let mk = |name, op, colon| {
            WordPart::Param(Box::new(ParamExp {
                name,
                index: None,
                op,
                colon,
                flags: None,
            }))
        };
        self.eat_bnl(0);
        let Some(c) = self.at(0) else {
            return self.eof_err("Syntax error: Missing '}'");
        };
        if c == b'}' {
            self.pos += 1;
            return Ok(mk(name, ParamOp::Plain, false));
        }
        if c == b'/' {
            // `${x/pattern/replacement}`: the pattern starts a fresh quoting
            // context, as that of `%` and `#` does.
            self.pos += 1;
            let how = match self.at(0) {
                Some(b'/') => Replace::All,
                Some(b'#') => Replace::Prefix,
                Some(b'%') => Replace::Suffix,
                _ => Replace::First,
            };
            if how != Replace::First {
                self.pos += 1;
            }
            let (pat, slash) = self.read_param_word_to(Ctx::Unquoted, Some(b'/'))?;
            let rep = if slash {
                self.read_param_word(ctx)?
            } else {
                Word::default()
            };
            return Ok(mk(name, ParamOp::Replace(how, pat, rep), false));
        }
        let colon = c == b':';
        if colon {
            self.pos += 1;
            self.eat_bnl(0);
        }
        let Some(c) = self.at(0) else {
            return self.eof_err("Syntax error: Missing '}'");
        };
        // `${x:offset:length}`. As in zsh, a letter after the `:` would
        // start a modifier (`${x:h}`), which luish doesn't have.
        if colon
            && !c.is_ascii_alphabetic()
            && !matches!(c, b'-' | b'=' | b'?' | b'+' | b'#' | b'%' | b'/' | b':' | b'}')
        {
            let (offset, more) = self.read_param_word_to(Ctx::DQuote, Some(b':'))?;
            let len = if more {
                Some(self.read_param_word_to(Ctx::DQuote, None)?.0)
            } else {
                None
            };
            if len.as_ref().is_some_and(|w| w.0.is_empty()) {
                return Ok(mk(name, ParamOp::Bad(Word::default()), true));
            }
            return Ok(mk(name, ParamOp::Substring(offset, len), true));
        }
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
            _ => {
                self.pos -= 1;
                return bad(self, name, colon);
            }
        };
        // Inside double quotes, the pattern of `%`/`#` starts a fresh quoting
        // context, while the word of the other operators stays quoted.
        let wctx = if pattern { Ctx::Unquoted } else { ctx };
        let word = self.read_param_word(wctx)?;
        Ok(mk(name, op(word), colon))
    }

    /// Reads the word in `${x-word}` up to the closing `}`.
    fn read_param_word(&mut self, ctx: Ctx) -> PResult<Word> {
        self.read_param_word_to(ctx, None).map(|(w, _)| w)
    }

    /// Reads a word up to the closing `}` or to an unquoted `stop`, and
    /// consumes that. Returns whether it was `stop`.
    fn read_param_word_to(&mut self, ctx: Ctx, stop: Option<u8>) -> PResult<(Word, bool)> {
        let mut parts = Vec::new();
        let mut lit = Vec::new();
        let at_stop = loop {
            let Some(c) = self.at(0) else {
                return self.eof_err("Syntax error: Missing '}'");
            };
            if c == b'}' || Some(c) == stop {
                self.pos += 1;
                break c != b'}';
            }
            // `\]` in a subscript is a `]` in the key (as in `a[x\]]=v`).
            if c == b'\\' && stop == Some(b']') && self.at(1) == Some(b']') {
                flush(&mut parts, &mut lit);
                parts.push(WordPart::Escaped(b']'));
                self.pos += 2;
                continue;
            }
            self.read_word_char(c, &mut parts, &mut lit, ctx)?;
        };
        flush(&mut parts, &mut lit);
        if ctx == Ctx::Unquoted {
            mark_leading_tilde(&mut parts);
        }
        Ok((Word(parts), at_stop))
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
        self.read_subst_list().map(WordPart::CmdSubst)
    }

    /// After `$(`, `<(` or `>(`: the list up to the matching `)`.
    fn read_subst_list(&mut self) -> PResult<Rc<List>> {
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
        Ok(Rc::new(list))
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
        sub.bareglobqual = self.bareglobqual;
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
                sub.bareglobqual = self.bareglobqual;
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

    /// If the next token, a command name, is an alias that is not already
    /// being expanded, substitute the alias text into the input. For a
    /// suffix alias, that is its value and the word.
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
            if crate::parser::is_reserved(&name) {
                return Ok(());
            }
            let aliases = self.aliases.clone();
            if let Some(a) = aliases.get(&name) {
                if self.is_active(&name) {
                    return Ok(());
                }
                let tok = self.peeked.take().unwrap();
                self.splice_alias(tok, name, &a.value, ends_in_blank(&a.value));
                continue;
            }
            let Some((suffix, value)) = aliases.for_suffix(&name) else {
                return Ok(());
            };
            let key = [b"\0", suffix].concat();
            if self.is_active(&key) {
                return Ok(());
            }
            let tok = self.peeked.take().unwrap();
            // As in zsh, a blank at the end of the value makes the word
            // after the name eligible (zsh's manual says it doesn't).
            self.splice_alias(tok, key, &[value, b" ", &name].concat(), ends_in_blank(value));
        }
    }

    /// Expands a global alias in the peeked token, whatever its position.
    fn expand_global(&mut self) -> PResult<()> {
        loop {
            let Some(Token { tok: Tok::Word(w), .. }) = &self.peeked else {
                return Ok(());
            };
            let Some(name) = w.as_literal() else {
                return Ok(());
            };
            let aliases = self.aliases.clone();
            let Some(a) = aliases.get(name).filter(|a| a.global) else {
                return Ok(());
            };
            if crate::parser::is_reserved(name) || self.is_active(name) {
                return Ok(());
            }
            let name = name.to_vec();
            let tok = self.peeked.take().unwrap();
            self.splice_alias(tok, name, &a.value, ends_in_blank(&a.value));
            self.peeked = Some(self.lex()?);
        }
    }

    fn is_active(&self, key: &[u8]) -> bool {
        self.active_aliases.iter().any(|(n, _)| n == key)
    }

    /// Replaces the token `tok` with `value`, the text of the alias `key`,
    /// and reads on from its start. If `blank`, the word after `value` is
    /// checked for aliases too.
    fn splice_alias(&mut self, tok: Token, key: Vec<u8>, value: &[u8], blank: bool) {
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
        self.active_aliases.push((key, tok.start + value.len()));
        if blank {
            self.alias_blank_end = Some(tok.start + value.len());
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
