//! Shell source text for an AST, which parses back to an equivalent tree.
//!
//! Unlike `cmdtext.rs` (dash's job text, which drops quoting), this keeps
//! everything that affects the meaning, so that functions can be saved and
//! read back (`__luish_internal savestate`). Backquotes become `$(...)`
//! (with a `$` before them escaped), and `<<-` becomes `<<` (its tabs were
//! already stripped); the layout is one command per line, indented by four
//! spaces.

use std::cell::Cell;

use crate::ast::*;
use crate::lexer::{AliasMap, is_name_char, is_valid_name};
use crate::parser::is_reserved;

/// The definition of a function, ending with a newline. A command name that
/// is one of `aliases` is quoted, so that reading the text back doesn't
/// expand it again. Also returns whether the text has a glob qualifier
/// (which reads back only under `setopt glob.bare_qualifiers`).
pub fn function(name: &[u8], body: &FunctionBody, aliases: &AliasMap) -> (Vec<u8>, bool) {
    let globqual = Cell::new(false);
    let mut p = Printer::new(0, aliases, &globqual);
    p.function(&[name], body);
    (p.finish(), globqual.get())
}

/// The text of a `__luish_cache` block, for the startup cache's key (so
/// that editing comments or the layout doesn't change it).
pub fn cache_block(block: &CacheBlock) -> Vec<u8> {
    let globqual = Cell::new(false);
    let aliases = AliasMap::default();
    let mut p = Printer::new(0, &aliases, &globqual);
    p.cache_block(block);
    p.finish()
}

struct Printer<'a> {
    out: Vec<u8>,
    indent: usize,
    aliases: &'a AliasMap,
    /// Set when a glob qualifier is written.
    globqual: &'a Cell<bool>,
    /// Here-document bodies (with their delimiters) to write after the
    /// next newline.
    heredocs: Vec<(Vec<u8>, Vec<u8>)>,
}

impl<'a> Printer<'a> {
    fn new(indent: usize, aliases: &'a AliasMap, globqual: &'a Cell<bool>) -> Printer<'a> {
        Printer {
            out: Vec::new(),
            indent,
            aliases,
            globqual,
            heredocs: Vec::new(),
        }
    }

    fn w(&mut self, s: &[u8]) {
        self.out.extend_from_slice(s);
    }

    /// Ends the line: writes pending here-document bodies, then indents the
    /// next line.
    fn nl(&mut self) {
        self.out.push(b'\n');
        self.flush_heredocs();
        self.out.resize(self.out.len() + 4 * self.indent, b' ');
    }

    fn flush_heredocs(&mut self) {
        for (delim, body) in std::mem::take(&mut self.heredocs) {
            self.out.extend_from_slice(&body);
            self.out.extend_from_slice(&delim);
            self.out.push(b'\n');
        }
    }

    /// The text, with a final newline if there are here-documents to end
    /// (or `always`).
    fn finish_with(mut self, always: bool) -> Vec<u8> {
        if always || !self.heredocs.is_empty() {
            self.out.push(b'\n');
            self.flush_heredocs();
        }
        self.out
    }

    fn finish(self) -> Vec<u8> {
        self.finish_with(true)
    }

    fn function(&mut self, names: &[impl AsRef<[u8]>], body: &FunctionBody) {
        match names {
            [name] if is_valid_name(name.as_ref()) && !is_reserved(name.as_ref()) => {
                self.w(name.as_ref());
                self.w(b"() ");
            }
            // Other names only read back after `function`.
            _ => {
                self.w(b"function ");
                for name in names {
                    self.w(name.as_ref());
                    self.w(b" ");
                }
                if !matches!(body.cmd, CompoundCommand::BraceGroup(_)) {
                    self.w(b"() ");
                }
            }
        }
        self.compound(&body.cmd);
        self.redirs(&body.redirs);
    }

    /// The commands of a list, each on its own line (a separator comes
    /// before each one, but not before the first unless `leading`).
    fn seq(&mut self, list: &List, leading: bool) {
        for (i, cc) in list.iter().enumerate() {
            if leading || i > 0 {
                self.nl();
            }
            self.and_or(&cc.list);
            if cc.async_ {
                self.w(b" &");
            }
        }
    }

    /// An indented list inside a compound command.
    fn block(&mut self, list: &List) {
        self.indent += 1;
        self.seq(list, true);
        self.indent -= 1;
    }

    /// The condition of `if`, `while` or `until`, followed by `then` or
    /// `do`: on the same line if it is a single command.
    fn cond(&mut self, list: &List, kw: &[u8]) {
        if let [cc] = list.as_slice()
            && !cc.async_
        {
            self.w(b" ");
            self.and_or(&cc.list);
            self.w(b"; ");
        } else {
            self.block(list);
            self.nl();
        }
        self.w(kw);
    }

    fn and_or(&mut self, ao: &AndOrList) {
        self.pipeline(&ao.first);
        for (kind, p) in &ao.rest {
            self.w(match kind {
                AndOr::And => b" && ",
                AndOr::Or => b" || ",
            });
            self.pipeline(p);
        }
    }

    fn pipeline(&mut self, p: &Pipeline) {
        if p.negated {
            self.w(b"! ");
        }
        for (i, c) in p.cmds.iter().enumerate() {
            if i > 0 {
                self.w(b" | ");
            }
            self.command(c);
        }
    }

    fn command(&mut self, cmd: &Command) {
        match cmd {
            Command::Simple(sc) => self.simple(sc),
            Command::Compound(cc, redirs) => {
                self.compound(cc);
                self.redirs(redirs);
            }
            Command::FunctionDef { names, body } => self.function(names, body),
            Command::Cache(block) => self.cache_block(block),
        }
    }

    fn cache_block(&mut self, block: &CacheBlock) {
        self.w(b"__luish_cache");
        if !block.env.is_empty() {
            self.w(b" env=(");
            self.w(&block.env.join(&b' '));
            self.w(b")");
        }
        if !block.files.is_empty() {
            self.w(b" files=(");
            for (i, f) in block.files.iter().enumerate() {
                if i > 0 {
                    self.w(b" ");
                }
                self.word(f);
            }
            self.w(b")");
        }
        self.w(b" {");
        self.block(&block.body);
        self.nl();
        self.w(b"}");
    }

    fn simple(&mut self, sc: &SimpleCommand) {
        let mut first = true;
        let mut sep = |p: &mut Self| {
            if !std::mem::take(&mut first) {
                p.w(b" ");
            }
        };
        for a in &sc.assigns {
            sep(self);
            self.w(&a.name);
            if let Some(index) = &a.index {
                self.w(b"[");
                self.parts(&index.0);
                self.w(b"]");
            }
            self.w(if a.append { b"+=" } else { b"=" });
            self.word(&a.value);
        }
        // A command named like a reserved word (`>f for`) keeps its
        // redirections first, where they make it a command.
        let reserved = sc.assigns.is_empty()
            && !sc.redirs.is_empty()
            && sc.words.first().and_then(|w| w.as_literal()).is_some_and(is_reserved);
        if reserved {
            for r in &sc.redirs {
                sep(self);
                self.redir(r);
            }
        }
        for (i, w) in sc.words.iter().enumerate() {
            sep(self);
            if i == 0 {
                if w.as_literal().is_some_and(|name| self.aliases.expands(name, true)) {
                    self.w(b"\\");
                }
                self.parts(&w.0);
            } else {
                self.word(w);
            }
        }
        if !reserved {
            for r in &sc.redirs {
                sep(self);
                self.redir(r);
            }
        }
    }

    fn redirs(&mut self, redirs: &[Redirect]) {
        for r in redirs {
            self.w(b" ");
            self.redir(r);
        }
    }

    fn redir(&mut self, r: &Redirect) {
        if let Some(fd) = r.fd {
            self.w(fd.to_string().as_bytes());
        }
        self.w(match r.kind {
            RedirKind::In => b"<",
            RedirKind::Out => b">",
            RedirKind::Append => b">>",
            RedirKind::Clobber => b">|",
            RedirKind::ReadWrite => b"<>",
            RedirKind::DupIn => b"<&",
            RedirKind::DupOut => b">&",
            RedirKind::HereDoc => b"<<",
            RedirKind::HereString => b"<<<",
        });
        match &r.target {
            RedirTarget::Word(w) => {
                // `<<(x)` would be read as a here-document.
                if matches!(w.0.first(), Some(WordPart::ProcSubst { .. })) {
                    self.w(b" ");
                }
                self.word(w)
            }
            RedirTarget::HereDoc(hd) => {
                let hd = hd.borrow();
                let mut body = match (hd.quoted, hd.body.0.as_slice()) {
                    (true, [WordPart::Literal(s)]) => s.clone(),
                    (true, []) => Vec::new(),
                    (_, [WordPart::DoubleQuoted(parts)]) => {
                        let mut p = Printer::new(0, self.aliases, self.globqual);
                        p.parts(parts);
                        p.finish_with(false)
                    }
                    _ => {
                        let mut p = Printer::new(0, self.aliases, self.globqual);
                        p.word(&hd.body);
                        p.finish_with(false)
                    }
                };
                if !body.is_empty() && body.last() != Some(&b'\n') {
                    body.push(b'\n');
                }
                let delim = heredoc_delim(&body);
                if hd.quoted {
                    self.w(b"'");
                    self.w(&delim);
                    self.w(b"'");
                } else {
                    self.w(&delim);
                }
                self.heredocs.push((delim, body));
            }
        }
    }

    fn compound(&mut self, cc: &CompoundCommand) {
        match cc {
            CompoundCommand::BraceGroup(l) => {
                self.w(b"{");
                self.block(l);
                self.nl();
                self.w(b"}");
            }
            CompoundCommand::Subshell(l) => {
                self.w(b"(");
                self.block(l);
                self.nl();
                self.w(b")");
            }
            CompoundCommand::If { conds, else_ } => {
                for (i, (cond, body)) in conds.iter().enumerate() {
                    if i > 0 {
                        self.nl();
                    }
                    self.w(if i == 0 { b"if" } else { b"elif" });
                    self.cond(cond, b"then");
                    self.block(body);
                }
                if let Some(e) = else_ {
                    self.nl();
                    self.w(b"else");
                    self.block(e);
                }
                self.nl();
                self.w(b"fi");
            }
            CompoundCommand::While { cond, body, until } => {
                self.w(if *until { b"until" } else { b"while" });
                self.cond(cond, b"do");
                self.block(body);
                self.nl();
                self.w(b"done");
            }
            CompoundCommand::For { var, words, body, .. } => {
                self.w(b"for ");
                self.w(var);
                match words {
                    Some(words) => {
                        self.w(b" in");
                        for w in words {
                            self.w(b" ");
                            self.word(w);
                        }
                        self.w(b"; do");
                    }
                    None => {
                        self.nl();
                        self.w(b"do");
                    }
                }
                self.block(body);
                self.nl();
                self.w(b"done");
            }
            CompoundCommand::Case { word, arms, .. } => {
                self.w(b"case ");
                self.word(word);
                self.w(b" in");
                self.indent += 1;
                for arm in arms {
                    self.nl();
                    // The `(` form, so that a pattern can't be read as `esac`.
                    self.w(b"(");
                    for (i, p) in arm.patterns.iter().enumerate() {
                        if i > 0 {
                            self.w(b" | ");
                        }
                        self.word(p);
                    }
                    self.w(b")");
                    self.block(&arm.body);
                    self.nl();
                    self.w(b";;");
                }
                self.indent -= 1;
                self.nl();
                self.w(b"esac");
            }
            CompoundCommand::Cond { expr, .. } => {
                self.w(b"[[ ");
                expr.write(&mut |p| match p {
                    CondPiece::Text(t) => self.w(t),
                    CondPiece::Word(w) => self.word(w),
                });
                self.w(b" ]]");
            }
        }
    }

    /// A word, quoted if it would be expanded as a global alias.
    fn word(&mut self, w: &Word) {
        if w.as_literal().is_some_and(|name| self.aliases.expands(name, false)) {
            self.w(b"\\");
        }
        self.parts(&w.0);
    }

    /// Word parts, as the lexer read them in their context: the text of a
    /// literal part is written as it was, since whatever needed quoting in
    /// it is in a quoted or escaped part.
    fn parts(&mut self, parts: &[WordPart]) {
        for (i, part) in parts.iter().enumerate() {
            match part {
                // A `$` before backquotes, which are written as `$(...)`,
                // would read as `$$`: it is escaped (which means the same).
                WordPart::Literal(s)
                    if s.ends_with(b"$") && matches!(parts.get(i + 1), Some(WordPart::CmdSubst(_))) =>
                {
                    self.w(&s[..s.len() - 1]);
                    self.w(b"\\$");
                }
                WordPart::Literal(s) => self.w(s),
                WordPart::SingleQuoted(s) => {
                    self.w(b"'");
                    self.w(s);
                    self.w(b"'");
                }
                WordPart::DoubleQuoted(inner) => {
                    self.w(b"\"");
                    self.parts(inner);
                    self.w(b"\"");
                }
                WordPart::Escaped(c) => self.w(&[b'\\', *c]),
                WordPart::Tilde(user) => {
                    self.w(b"~");
                    self.w(user);
                }
                WordPart::Param(pe) => {
                    // `$x` followed by literal text that would extend the name
                    // needs braces.
                    let joins = matches!(parts.get(i + 1), Some(WordPart::Literal(s)) if s.first().is_some_and(|&c| is_name_char(c)));
                    self.param(pe, joins);
                }
                WordPart::CmdSubst(list) => self.cmdsubst(b"$(", list),
                WordPart::ProcSubst { output, list } => self.cmdsubst(if *output { b">(" } else { b"<(" }, list),
                WordPart::Arith(w) => {
                    self.w(b"$((");
                    self.word(w);
                    self.w(b"))");
                }
                WordPart::GlobQual(q) => {
                    self.globqual.set(true);
                    self.w(b"(");
                    self.w(q);
                    self.w(b")");
                }
                WordPart::Array(items) => {
                    self.w(b"(");
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            self.w(b" ");
                        }
                        match &item.key {
                            Some(k) => {
                                self.w(b"[");
                                self.parts(&k.0);
                                self.w(b"]=");
                                self.parts(&item.value.0);
                            }
                            None => self.word(&item.value),
                        }
                    }
                    self.w(b")");
                }
            }
        }
    }

    fn param(&mut self, pe: &ParamExp, joins: bool) {
        let short = match &pe.name {
            _ if pe.index.is_some() || pe.flags.is_some() => false,
            ParamName::Var(_) => !joins,
            ParamName::Positional(n) => *n < 10 && !joins,
            ParamName::Special(_) => true,
            ParamName::Indirect(_) => false,
        };
        if pe.op == ParamOp::Plain && short {
            self.w(b"$");
            self.param_name(pe);
            return;
        }
        if pe.op == ParamOp::Names {
            self.w(b"${!");
            self.name(&pe.name);
            self.w(if pe.index == Some(Index::Star) { b"*}" } else { b"@}" });
            return;
        }
        self.w(match pe.op {
            ParamOp::Length => b"${#",
            ParamOp::Keys => b"${!",
            _ => b"${",
        });
        if let Some(flags) = &pe.flags {
            self.w(b"(");
            self.w(&flags.text);
            self.w(b")");
        }
        self.param_name(pe);
        let (op, word): (&[u8], _) = match &pe.op {
            ParamOp::Plain | ParamOp::Length | ParamOp::Keys | ParamOp::Names => {
                self.w(b"}");
                return;
            }
            ParamOp::Default(w) => (b"-", w),
            ParamOp::Assign(w) => (b"=", w),
            ParamOp::Error(w) => (b"?", w),
            ParamOp::Alternative(w) => (b"+", w),
            ParamOp::RemoveSmallestSuffix(w) => (b"%", w),
            ParamOp::RemoveLargestSuffix(w) => (b"%%", w),
            ParamOp::RemoveSmallestPrefix(w) => (b"#", w),
            ParamOp::RemoveLargestPrefix(w) => (b"##", w),
            ParamOp::Bad(text) => {
                if pe.colon {
                    self.w(b":");
                }
                self.w(text);
                self.w(b"}");
                return;
            }
            ParamOp::Substring(offset, len) => {
                // The text of a negative offset starts with a space or `(`.
                self.w(b":");
                self.word(offset);
                if let Some(len) = len {
                    self.w(b":");
                    self.word(len);
                }
                self.w(b"}");
                return;
            }
            ParamOp::Replace(how, pat, rep) => {
                self.w(how.text());
                self.word(pat);
                // Without a replacement, no `/`: `${x//}` would be `//`
                // with an empty pattern.
                if !rep.0.is_empty() {
                    self.w(b"/");
                    self.word(rep);
                }
                self.w(b"}");
                return;
            }
            ParamOp::Modify(mods) => {
                for m in mods {
                    self.w(b":");
                    self.w(m.text().as_bytes());
                }
                self.w(b"}");
                return;
            }
        };
        if pe.colon {
            self.w(b":");
        }
        self.w(op);
        self.word(word);
        self.w(b"}");
    }

    /// The name of a parameter expansion, with its subscript.
    fn param_name(&mut self, pe: &ParamExp) {
        self.name(&pe.name);
        match &pe.index {
            None => {}
            Some(Index::At) => self.w(b"[@]"),
            Some(Index::Star) => self.w(b"[*]"),
            Some(Index::Expr(w)) => {
                self.w(b"[");
                self.word(w);
                self.w(b"]");
            }
            Some(Index::Slice(s)) => {
                self.w(b"[");
                if let Some(w) = &s.0 {
                    self.word(w);
                }
                self.w(b"..");
                if let Some(w) = &s.1 {
                    self.word(w);
                }
                self.w(b"]");
            }
        }
    }

    fn name(&mut self, name: &ParamName) {
        match name {
            ParamName::Var(n) => self.w(n),
            ParamName::Positional(n) => self.w(n.to_string().as_bytes()),
            ParamName::Special(c) => self.w(&[*c]),
            ParamName::Indirect(n) => {
                self.w(b"!");
                self.name(n);
            }
        }
    }

    fn cmdsubst(&mut self, open: &[u8], list: &List) {
        let mut p = Printer::new(self.indent + 1, self.aliases, self.globqual);
        p.seq(list, false);
        let text = p.finish_with(false);
        if !text.contains(&b'\n') {
            // `$( (...) )`, not `$((`
            self.w(open);
            if text.first() == Some(&b'(') {
                self.w(b" ");
            }
            self.w(&text);
            self.w(b")");
            return;
        }
        // Written directly rather than with `nl`, which would write the
        // bodies of here-documents started before the substitution.
        self.w(open);
        self.w(b"\n");
        self.out.resize(self.out.len() + 4 * (self.indent + 1), b' ');
        self.w(&text);
        if text.last() != Some(&b'\n') {
            self.out.push(b'\n');
        }
        self.out.resize(self.out.len() + 4 * self.indent, b' ');
        self.w(b")");
    }
}

/// A here-document delimiter that isn't a line of `body`.
fn heredoc_delim(body: &[u8]) -> Vec<u8> {
    let mut n = 0;
    loop {
        let delim = if n == 0 {
            b"EOF".to_vec()
        } else {
            format!("EOF{n}").into_bytes()
        };
        if !body.split(|&c| c == b'\n').any(|l| l == delim.as_slice()) {
            return delim;
        }
        n += 1;
    }
}

/// What [`walk_lines`] does with what it visits.
pub trait LineVisitor {
    /// Whether the walk changes the tree: here-document bodies, which may
    /// be shared with the tree being run, are only read otherwise.
    const WRITES: bool;
    fn line(&mut self, n: &mut u32);
    /// The parts of a word, after those nested in them.
    fn parts(&mut self, _: &mut Vec<WordPart>) {}
}

/// Visits every line number in the list, in an order that is the same for
/// a tree and the tree its printed text parses to (its structure is the
/// same, only the line numbers differ). Shared lists in words are copied
/// before they are visited (`Rc::make_mut`).
pub fn walk_lines<V: LineVisitor>(list: &mut List, v: &mut V) {
    for cc in list {
        let ao = &mut cc.list;
        for p in std::iter::once(&mut ao.first).chain(ao.rest.iter_mut().map(|(_, p)| p)) {
            for c in &mut p.cmds {
                match c {
                    Command::Simple(sc) => {
                        v.line(&mut sc.lineno);
                        for a in &mut sc.assigns {
                            a.index.iter_mut().for_each(|w| word(w, v));
                            word(&mut a.value, v);
                        }
                        words(&mut sc.words, v);
                        redirs(&mut sc.redirs, v);
                    }
                    Command::Compound(cc, rs) => {
                        compound(cc, v);
                        redirs(rs, v);
                    }
                    Command::FunctionDef { body, .. } => walk_body_lines(std::rc::Rc::make_mut(body), v),
                    Command::Cache(block) => {
                        let b = std::rc::Rc::make_mut(block);
                        v.line(&mut b.lineno);
                        words(&mut b.files, v);
                        walk_lines(&mut b.body, v);
                    }
                }
            }
        }
    }
}

/// [`walk_lines`] for the body of a function.
pub fn walk_body_lines<V: LineVisitor>(body: &mut FunctionBody, v: &mut V) {
    compound(&mut body.cmd, v);
    redirs(&mut body.redirs, v);
}

fn words<V: LineVisitor>(ws: &mut [Word], v: &mut V) {
    ws.iter_mut().for_each(|w| word(w, v));
}

fn word<V: LineVisitor>(w: &mut Word, v: &mut V) {
    w.0.iter_mut().for_each(|p| part(p, v));
    v.parts(&mut w.0);
}

fn part<V: LineVisitor>(p: &mut WordPart, v: &mut V) {
    match p {
        WordPart::DoubleQuoted(ps) => {
            ps.iter_mut().for_each(|p| part(p, v));
            v.parts(ps);
        }
        WordPart::CmdSubst(l) | WordPart::ProcSubst { list: l, .. } => walk_lines(std::rc::Rc::make_mut(l), v),
        WordPart::Arith(w) => word(w, v),
        WordPart::Array(items) => {
            for item in items {
                item.key.iter_mut().for_each(|w| word(w, v));
                word(&mut item.value, v);
            }
        }
        WordPart::Param(pe) => {
            match &mut pe.index {
                Some(Index::Expr(w)) => word(w, v),
                Some(Index::Slice(ends)) => {
                    ends.0.iter_mut().for_each(|w| word(w, v));
                    ends.1.iter_mut().for_each(|w| word(w, v));
                }
                _ => {}
            }
            param_op(&mut pe.op, v);
        }
        _ => {}
    }
}

fn param_op<V: LineVisitor>(op: &mut ParamOp, v: &mut V) {
    match op {
        ParamOp::Plain | ParamOp::Length | ParamOp::Keys | ParamOp::Names | ParamOp::Modify(_) | ParamOp::Bad(_) => {}
        ParamOp::Default(w)
        | ParamOp::Assign(w)
        | ParamOp::Error(w)
        | ParamOp::Alternative(w)
        | ParamOp::RemoveSmallestSuffix(w)
        | ParamOp::RemoveLargestSuffix(w)
        | ParamOp::RemoveSmallestPrefix(w)
        | ParamOp::RemoveLargestPrefix(w) => word(w, v),
        ParamOp::Substring(offset, len) => {
            word(offset, v);
            len.iter_mut().for_each(|w| word(w, v));
        }
        ParamOp::Replace(_, pat, rep) => {
            word(pat, v);
            word(rep, v);
        }
    }
}

fn redirs<V: LineVisitor>(rs: &mut [Redirect], v: &mut V) {
    for r in rs {
        match &mut r.target {
            RedirTarget::Word(w) => word(w, v),
            RedirTarget::HereDoc(hd) if V::WRITES => word(&mut hd.borrow_mut().body, v),
            RedirTarget::HereDoc(hd) => word(&mut hd.borrow().body.clone(), v),
        }
    }
}

fn compound<V: LineVisitor>(cc: &mut CompoundCommand, v: &mut V) {
    match cc {
        CompoundCommand::BraceGroup(l) | CompoundCommand::Subshell(l) => walk_lines(l, v),
        CompoundCommand::If { conds, else_ } => {
            for (c, b) in conds {
                walk_lines(c, v);
                walk_lines(b, v);
            }
            if let Some(e) = else_ {
                walk_lines(e, v);
            }
        }
        CompoundCommand::While { cond, body, .. } => {
            walk_lines(cond, v);
            walk_lines(body, v);
        }
        CompoundCommand::For {
            words: ws,
            body,
            lineno,
            ..
        } => {
            v.line(lineno);
            if let Some(ws) = ws {
                words(ws, v);
            }
            walk_lines(body, v);
        }
        CompoundCommand::Case { word: w, arms, lineno } => {
            v.line(lineno);
            word(w, v);
            for a in arms {
                words(&mut a.patterns, v);
                walk_lines(&mut a.body, v);
            }
        }
        CompoundCommand::Cond { expr, lineno } => {
            v.line(lineno);
            expr.words_mut(&mut |w| word(w, v));
        }
    }
}

/// The line numbers of a function's body, in the order of [`walk_lines`],
/// for the function to keep them when its printed text is read back
/// ([`set_body_lines`]).
pub fn body_lines(body: &FunctionBody) -> Vec<u32> {
    struct Collect(Vec<u32>);
    impl LineVisitor for Collect {
        const WRITES: bool = false;
        fn line(&mut self, n: &mut u32) {
            self.0.push(*n);
        }
    }
    let mut c = Collect(Vec::new());
    walk_body_lines(&mut body.clone(), &mut c);
    c.0
}

/// Gives the body of a function read back from its printed text the line
/// numbers of the original ([`body_lines`]). False if they don't fit (its
/// text came from elsewhere), when its line numbers are left wrong.
pub fn set_body_lines(body: &mut FunctionBody, lines: &[u32]) -> bool {
    struct Set<'a>(std::slice::Iter<'a, u32>, bool);
    impl LineVisitor for Set<'_> {
        const WRITES: bool = true;
        fn line(&mut self, n: &mut u32) {
            match self.0.next() {
                Some(l) => *n = *l,
                None => self.1 = false,
            }
        }
    }
    let mut set = Set(lines.iter(), true);
    walk_body_lines(body, &mut set);
    set.1 && set.0.next().is_none()
}

/// Clears line numbers, which differ between the original and the
/// printed text (for the round-trip tests, and the fuzzer's). Also turns
/// the `\$` written before `$(...)` (`parts`) back into a literal `$`.
#[cfg(any(test, fuzzing))]
pub fn strip_lines(list: &mut List) {
    struct Strip;
    impl LineVisitor for Strip {
        const WRITES: bool = true;
        fn line(&mut self, n: &mut u32) {
            *n = 0;
        }
        fn parts(&mut self, ps: &mut Vec<WordPart>) {
            for i in 0..ps.len() {
                if matches!(ps[i], WordPart::Escaped(b'$')) && matches!(ps.get(i + 1), Some(WordPart::CmdSubst(_))) {
                    ps[i] = WordPart::Literal(b"$".to_vec());
                }
            }
            // Adjacent literals, as the lexer reads them.
            let mut merged: Vec<WordPart> = Vec::with_capacity(ps.len());
            for p in ps.drain(..) {
                match (merged.last_mut(), p) {
                    (Some(WordPart::Literal(a)), WordPart::Literal(b)) => a.extend(b),
                    (_, p) => merged.push(p),
                }
            }
            *ps = merged;
        }
    }
    walk_lines(list, &mut Strip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Parser;

    fn parse(src: &[u8]) -> List {
        Parser::new(src.to_vec(), 1, true).parse_all().expect("parses")
    }

    /// The function defined by `src`, printed.
    fn print(src: &[u8], aliases: &AliasMap) -> String {
        let list = parse(src);
        let Command::FunctionDef { names, body } = &list[0].list.first.cmds[0] else {
            panic!("not a function: {}", String::from_utf8_lossy(src));
        };
        let globqual = Cell::new(false);
        let mut p = Printer::new(0, aliases, &globqual);
        p.function(names, body);
        String::from_utf8(p.finish()).unwrap()
    }

    /// Printing `src` gives text that parses to the same tree, to which
    /// the original line numbers can be given back.
    fn round_trip(src: &str) -> String {
        let printed = print(src.as_bytes(), &AliasMap::default());
        let mut a = parse(src.as_bytes());
        let mut b = parse(printed.as_bytes());
        if let (Command::FunctionDef { body: x, .. }, Command::FunctionDef { body: y, .. }) =
            (&a[0].list.first.cmds[0], &mut b[0].list.first.cmds[0])
        {
            let lines = body_lines(x);
            assert!(set_body_lines(std::rc::Rc::make_mut(y), &lines), "{printed}");
            assert_eq!(body_lines(y), lines);
        }
        strip_lines(&mut a);
        strip_lines(&mut b);
        assert_eq!(a, b, "\nsource:\n{src}\nprinted:\n{printed}");
        printed
    }

    #[test]
    fn cache_block() {
        assert_eq!(
            round_trip("f() { __luish_cache env=(A B) files=(\"$H\"/a ~/b\\ c) { x=1; }; __luish_cache { :; }; }"),
            "f() {\n    __luish_cache env=(A B) files=(\"$H\"/a ~/b\\ c) {\n        x=1\n    }\n    __luish_cache {\n        :\n    }\n}\n"
        );
    }

    #[test]
    fn dollar_before_backquotes() {
        // `$` then `$(...)` would read as `$$`.
        assert_eq!(
            round_trip("f() { echo a$`b` \"$`c`\"; }"),
            "f() {\n    echo a\\$$(b) \"\\$$(c)\"\n}\n"
        );
    }

    #[test]
    fn process_substitution() {
        assert_eq!(
            round_trip("f() { diff <(sort a) <(sort b) >(cat -n)x; }"),
            "f() {\n    diff <(sort a) <(sort b) >(cat -n)x\n}\n"
        );
        assert_eq!(
            round_trip("f() { cat <(( echo a ); b); }"),
            "f() {\n    cat <(\n        (\n            echo a\n        )\n        b\n    )\n}\n"
        );
        // A subshell first must not read as `<((`.
        round_trip("f() { cat <(( echo a )); }");
        assert!(round_trip("f() { while read x; do :; done < <(cmd); }").contains("< <(cmd)"));
    }

    #[test]
    fn layout() {
        assert_eq!(
            round_trip("f() { if a; then b; elif c; d; then :; else e & fi; }"),
            "f() {\n    if a; then\n        b\n    elif\n        c\n        d\n    then\n        :\n    else\n        e &\n    fi\n}\n"
        );
        assert_eq!(
            round_trip("f() { for i in a \"$@\"; do echo $i; done; for j do :; done; }"),
            "f() {\n    for i in a \"$@\"; do\n        echo $i\n    done\n    for j\n    do\n        :\n    done\n}\n"
        );
        assert_eq!(
            round_trip("f() { case $1 in a|esac) x;; *) ;; esac; }"),
            "f() {\n    case $1 in\n        (a | esac)\n            x\n        ;;\n        (*)\n        ;;\n    esac\n}\n"
        );
        assert_eq!(
            round_trip("f() ( cd /; ls ) >out 2>&1"),
            "f() (\n    cd /\n    ls\n) >out 2>&1\n"
        );
        assert_eq!(round_trip("f() echo hi"), "f() {\n    echo hi\n}\n");
        assert_eq!(
            round_trip("f() { [[ -f $1 && ( $2 = a* || ! $3 =~ ^(a|b c)$ ) ]] >/dev/null; }"),
            "f() {\n    [[ -f $1 && ( $2 == a* || ! $3 =~ ^(a|b c)$ ) ]] >/dev/null\n}\n"
        );
        assert_eq!(
            round_trip("f() { [[ a && (b || c) ]]; }"),
            "f() {\n    [[ -n a && ( -n b || -n c ) ]]\n}\n"
        );
        assert_eq!(round_trip("f() [[ \"<\" < '>' ]]"), "f() [[ \"<\" < '>' ]]\n");
        // Names that only `function` can define.
        assert_eq!(round_trip("function a-b { :; }"), "function a-b {\n    :\n}\n");
        assert_eq!(round_trip("function if { :; }"), "function if {\n    :\n}\n");
        assert_eq!(round_trip("function a.b () ( : )"), "function a.b () (\n    :\n)\n");
        assert_eq!(
            round_trip("f() { function g h { :; }; }"),
            "f() {\n    function g h {\n        :\n    }\n}\n"
        );
    }

    #[test]
    fn words() {
        round_trip(
            r#"f() { a=1 b=~/x:~u c="$a"'b'\c echo ~ ~u/x "${x}y" ${x}_ "$1" ${10} ${1}0 $0 $# $? $$ $! $- $@ $*; }"#,
        );
        round_trip(r#"f() { echo "a\b" "\$\`\"\\" '\' $ a$ "$" $'x' \\ "${#x}" ${##} ${#-} ${#-x}; }"#);
        round_trip(
            r#"f() { echo ${x-a b} ${x:-"q"} ${x=~} ${x?err} ${x:+$y} ${x%.*} ${x%%/*} ${x#"$p"} ${x##*/} "${x-a\}b}"; }"#,
        );
        round_trip(r#"f() { echo ${x:foo} ${}; }"#);
        round_trip(r#"f() { echo ${``} "${`echo \`a\` \$b \\c`x}" ${:``}; }"#);
        round_trip(
            r#"f() { echo ${x:1} ${x: -1:$n} ${x:(-2)} "${@:2:1}" ${x/a/b} ${x//\//"*"} ${x/#a} "${x/%$p/~}" ${x/} ${x//}; }"#,
        );
        round_trip(r#"f() { echo $((1 + $x * (2 - y))) $(( $(echo 1) )) `echo a` "`echo \"b\"`"; }"#);
        round_trip(
            r#"f() { a=(x "y z" $(echo w)) b+=() c[$i+1]=q d[2]+=r e+=s; local g=(1 2); echo ${a[1]} "${a[@]}" ${#a[*]} ${a[i]:-x} ${a[@]/a/b} "${!a[@]}" ${!h[*]}; }"#,
        );
        round_trip("f() { x=$(a; b) y=$( (sub) ) z=$(case a in a) :;; esac); }");
        round_trip("f() { echo $(if a; then b; fi) \"$(for i in 1; do :; done)\"; }");
        round_trip("f() { ! a | b && c || { d; } & }");
        round_trip("f() { g() { echo in g; }; exec 3<>f 4<&3 5>&- >|c >>d <e; }");
        assert_eq!(
            round_trip("f() { >s for x; <a } 2>&1; x=1 if >b; }"),
            "f() {\n    >s for x\n    <a 2>&1 }\n    x=1 if >b\n}\n"
        );
    }

    #[test]
    fn heredocs() {
        // No body before the `)`: an empty one.
        round_trip("f() {\n x=$(cat <<E) y=$(<<'E')\n}");
        let p = round_trip("f() {\n cat <<A; cat <<-'B' | cat <<\"C\"\nx $y \\$ `z`\nA\n\tq $r\n\tB\n'c'\nC\n}");
        assert_eq!(
            p,
            "f() {\n    cat <<EOF\nx $y \\$ $(z)\nEOF\n    cat <<'EOF' | cat <<'EOF'\nq $r\nEOF\n'c'\nEOF\n}\n"
        );
        // A body line that is the default delimiter.
        let p = round_trip("f() {\ncat <<X\nEOF\nX\n}");
        assert!(p.contains("<<EOF1\nEOF\nEOF1\n"), "{p}");
        round_trip("f() {\necho $(cat <<X\ninner\nX\n) after\n}");
        round_trip("f() {\ncat <<X >out; echo\nX\n} 2>err <<Y\nbody\nY");
        round_trip("f() {\ncat <<X\nX\n}");
    }

    #[test]
    fn alias_names_quoted() {
        let mut aliases = AliasMap::default();
        aliases.insert(b"ls".to_vec(), b"ls -F".to_vec(), false);
        aliases.insert(b"G".to_vec(), b"| grep".to_vec(), true);
        aliases.insert_suffix(b"txt".to_vec(), b"less".to_vec());
        assert_eq!(
            print(b"f() { ls; echo ls; }", &aliases),
            "f() {\n    \\ls\n    echo ls\n}\n"
        );
        assert_eq!(
            print(b"f() { G G >G; a.txt a.txt; }", &aliases),
            "f() {\n    \\G \\G >\\G\n    \\a.txt a.txt\n}\n"
        );
    }
}
