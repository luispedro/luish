//! Abstract syntax tree produced by the parser.

use std::cell::RefCell;
use std::rc::Rc;

/// One line of input: a sequence of and-or lists, each optionally async.
pub type List = Vec<CompleteCommand>;

#[derive(Debug, Clone, PartialEq)]
pub struct CompleteCommand {
    pub list: AndOrList,
    pub async_: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AndOrList {
    pub first: Pipeline,
    pub rest: Vec<(AndOr, Pipeline)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndOr {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub negated: bool,
    pub cmds: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Simple(SimpleCommand),
    Compound(CompoundCommand, Vec<Redirect>),
    /// Several names only with `function` (as in zsh), each defined with the
    /// same body.
    FunctionDef {
        names: Vec<Vec<u8>>,
        body: Rc<FunctionBody>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionBody {
    pub cmd: CompoundCommand,
    pub redirs: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleCommand {
    pub assigns: Vec<Assign>,
    pub words: Vec<Word>,
    pub redirs: Vec<Redirect>,
    pub lineno: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    pub name: Vec<u8>,
    pub value: Word,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompoundCommand {
    BraceGroup(List),
    Subshell(List),
    If {
        conds: Vec<(List, List)>,
        else_: Option<List>,
    },
    While {
        cond: List,
        body: List,
        until: bool,
    },
    For {
        var: Vec<u8>,
        words: Option<Vec<Word>>,
        body: List,
        lineno: u32,
    },
    Case {
        word: Word,
        arms: Vec<CaseArm>,
        lineno: u32,
    },
    /// `[[ ... ]]`, as in zsh and bash.
    Cond {
        expr: CondExpr,
        lineno: u32,
    },
}

/// The expression of `[[ ... ]]`.
#[derive(Debug, Clone, PartialEq)]
pub enum CondExpr {
    Not(Box<CondExpr>),
    And(Box<CondExpr>, Box<CondExpr>),
    Or(Box<CondExpr>, Box<CondExpr>),
    /// `-f word` and the like, by the operator's letter. A lone word is
    /// `-n word`.
    Unary(u8, Word),
    Binary(CondOp, Word, Word),
}

impl CondExpr {
    /// How tightly the expression binds: `||`, then `&&`, then the rest.
    /// An operand that binds less tightly than its operator is written in
    /// parentheses.
    pub fn prec(&self) -> u8 {
        match self {
            CondExpr::Or(..) => 0,
            CondExpr::And(..) => 1,
            _ => 2,
        }
    }

    /// Writes the expression as it reads back (without `[[` and `]]`),
    /// passing text and words to `f`.
    pub fn write(&self, f: &mut dyn FnMut(CondPiece<'_>)) {
        let operand = |e: &CondExpr, f: &mut dyn FnMut(CondPiece<'_>)| {
            if e.prec() < self.prec() {
                f(CondPiece::Text(b"( "));
                e.write(f);
                f(CondPiece::Text(b" )"));
            } else {
                e.write(f);
            }
        };
        match self {
            CondExpr::Not(a) => {
                f(CondPiece::Text(b"! "));
                operand(a, f);
            }
            CondExpr::And(a, b) | CondExpr::Or(a, b) => {
                operand(a, f);
                f(CondPiece::Text(if matches!(self, CondExpr::And(..)) {
                    b" && "
                } else {
                    b" || "
                }));
                operand(b, f);
            }
            CondExpr::Unary(op, w) => {
                f(CondPiece::Text(&[b'-', *op, b' ']));
                f(CondPiece::Word(w));
            }
            CondExpr::Binary(op, a, b) => {
                f(CondPiece::Word(a));
                f(CondPiece::Text(b" "));
                f(CondPiece::Text(op.text().as_bytes()));
                f(CondPiece::Text(b" "));
                f(CondPiece::Word(b));
            }
        }
    }

    #[cfg(test)]
    pub fn words_mut(&mut self, f: &mut dyn FnMut(&mut Word)) {
        match self {
            CondExpr::Not(a) => a.words_mut(f),
            CondExpr::And(a, b) | CondExpr::Or(a, b) => {
                a.words_mut(f);
                b.words_mut(f);
            }
            CondExpr::Unary(_, w) => f(w),
            CondExpr::Binary(_, a, b) => {
                f(a);
                f(b);
            }
        }
    }
}

/// A piece of the text of a `[[ ... ]]` expression (`CondExpr::write`).
pub enum CondPiece<'a> {
    Text(&'a [u8]),
    Word(&'a Word),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondOp {
    /// `=` or `==`: the right side is a pattern.
    Match,
    NoMatch,
    /// `=~`: the right side is an extended regular expression.
    Regex,
    Less,
    Greater,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Nt,
    Ot,
    Ef,
}

impl CondOp {
    pub fn from_text(s: &[u8]) -> Option<CondOp> {
        Some(match s {
            b"=" | b"==" => CondOp::Match,
            b"!=" => CondOp::NoMatch,
            b"=~" => CondOp::Regex,
            b"<" => CondOp::Less,
            b">" => CondOp::Greater,
            b"-eq" => CondOp::Eq,
            b"-ne" => CondOp::Ne,
            b"-lt" => CondOp::Lt,
            b"-le" => CondOp::Le,
            b"-gt" => CondOp::Gt,
            b"-ge" => CondOp::Ge,
            b"-nt" => CondOp::Nt,
            b"-ot" => CondOp::Ot,
            b"-ef" => CondOp::Ef,
            _ => return None,
        })
    }

    pub fn text(self) -> &'static str {
        match self {
            CondOp::Match => "==",
            CondOp::NoMatch => "!=",
            CondOp::Regex => "=~",
            CondOp::Less => "<",
            CondOp::Greater => ">",
            CondOp::Eq => "-eq",
            CondOp::Ne => "-ne",
            CondOp::Lt => "-lt",
            CondOp::Le => "-le",
            CondOp::Gt => "-gt",
            CondOp::Ge => "-ge",
            CondOp::Nt => "-nt",
            CondOp::Ot => "-ot",
            CondOp::Ef => "-ef",
        }
    }
}

/// The letters of the unary operators of `[[ ... ]]`: `test`'s, and zsh's
/// `-a` (as `-e`), `-o` (an option is on), `-v` (a variable is set) and
/// `-N` (modified since last read).
pub const COND_UNARY: &[u8] = b"abcdefghknoprstuvwxzLOGSN";

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: List,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Word(pub Vec<WordPart>);

#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    /// Unquoted literal text.
    Literal(Vec<u8>),
    SingleQuoted(Vec<u8>),
    DoubleQuoted(Vec<WordPart>),
    /// Backslash-quoted byte.
    Escaped(u8),
    /// `~` or `~user`.
    Tilde(Vec<u8>),
    Param(Box<ParamExp>),
    /// `$(...)` and `` `...` ``, parsed eagerly.
    CmdSubst(Rc<List>),
    /// `$((...))`: the text is expanded first, then evaluated.
    Arith(Word),
    /// The text inside a trailing `(...)` glob qualifier (only lexed under
    /// `setopt glob.bare_qualifiers`). Always the last part of a word.
    GlobQual(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamExp {
    pub name: ParamName,
    pub op: ParamOp,
    pub colon: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamName {
    Var(Vec<u8>),
    Positional(usize),
    /// One of `@ * # ? - $ ! 0`.
    Special(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamOp {
    Plain,
    Length,
    Default(Word),
    Assign(Word),
    Error(Word),
    Alternative(Word),
    RemoveSmallestSuffix(Word),
    RemoveLargestSuffix(Word),
    RemoveSmallestPrefix(Word),
    RemoveLargestPrefix(Word),
    /// Not a valid substitution (such as bash's `${x//a/b}`). As in dash,
    /// this is an error only when it is expanded; the word is the rest of
    /// the text up to `}`.
    Bad(Word),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Redirect {
    pub fd: Option<u32>,
    pub kind: RedirKind,
    pub target: RedirTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirKind {
    In,
    Out,
    Append,
    Clobber,
    ReadWrite,
    DupIn,
    DupOut,
    HereDoc,
}

impl RedirKind {
    pub fn default_fd(self) -> u32 {
        match self {
            RedirKind::In | RedirKind::ReadWrite | RedirKind::DupIn | RedirKind::HereDoc => 0,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RedirTarget {
    Word(Word),
    HereDoc(Rc<RefCell<HereDocBody>>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HereDocBody {
    pub body: Word,
    /// A quoted delimiter means the body is not expanded.
    pub quoted: bool,
}

impl Word {
    /// The word's bytes if it is a single unquoted literal (used for
    /// reserved words, aliases, and function names).
    pub fn as_literal(&self) -> Option<&[u8]> {
        match self.0.as_slice() {
            [WordPart::Literal(s)] => Some(s),
            _ => None,
        }
    }
}
