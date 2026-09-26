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
    FunctionDef { name: Vec<u8>, body: Rc<FunctionBody> },
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
}

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
