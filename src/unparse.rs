//! Shell source text for an AST, which parses back to an equivalent tree.
//!
//! Unlike `cmdtext.rs` (dash's job text, which drops quoting), this keeps
//! everything that affects the meaning, so that functions can be saved and
//! read back (`__luish_internal savestate`). Backquotes become `$(...)` and `<<-` becomes
//! `<<` (its tabs were already stripped); the layout is one command per
//! line, indented by four spaces.

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
        }
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
            self.w(b"=");
            self.word(&a.value);
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
        for r in &sc.redirs {
            sep(self);
            self.redir(r);
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
        });
        match &r.target {
            RedirTarget::Word(w) => self.word(w),
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
                WordPart::CmdSubst(list) => self.cmdsubst(list),
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
            }
        }
    }

    fn param(&mut self, pe: &ParamExp, joins: bool) {
        let name = match &pe.name {
            ParamName::Var(n) => n.clone(),
            ParamName::Positional(n) => n.to_string().into_bytes(),
            ParamName::Special(c) => vec![*c],
        };
        let (op, word): (&[u8], _) = match &pe.op {
            ParamOp::Plain => {
                let short = match &pe.name {
                    ParamName::Var(_) => !joins,
                    ParamName::Positional(n) => *n < 10 && !joins,
                    ParamName::Special(_) => true,
                };
                if short {
                    self.w(b"$");
                    self.w(&name);
                } else {
                    self.w(b"${");
                    self.w(&name);
                    self.w(b"}");
                }
                return;
            }
            ParamOp::Length => {
                self.w(b"${#");
                self.w(&name);
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
            ParamOp::Bad(w) => (b"", w),
        };
        self.w(b"${");
        self.w(&name);
        if pe.colon {
            self.w(b":");
        }
        self.w(op);
        self.word(word);
        self.w(b"}");
    }

    fn cmdsubst(&mut self, list: &List) {
        let mut p = Printer::new(self.indent + 1, self.aliases, self.globqual);
        p.seq(list, false);
        let text = p.finish_with(false);
        if !text.contains(&b'\n') {
            // `$( (...) )`, not `$((`
            self.w(if text.first() == Some(&b'(') { b"$( " } else { b"$(" });
            self.w(&text);
            self.w(b")");
            return;
        }
        // Written directly rather than with `nl`, which would write the
        // bodies of here-documents started before the substitution.
        self.w(b"$(\n");
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

    /// Clears line numbers, which differ between the original and the
    /// printed text.
    fn strip_lines(list: &mut List) {
        fn words(ws: &mut [Word]) {
            ws.iter_mut().for_each(word);
        }
        fn word(w: &mut Word) {
            w.0.iter_mut().for_each(part);
        }
        fn part(p: &mut WordPart) {
            match p {
                WordPart::DoubleQuoted(ps) => ps.iter_mut().for_each(part),
                WordPart::CmdSubst(l) => strip_lines(std::rc::Rc::make_mut(l)),
                WordPart::Arith(w) => word(w),
                WordPart::Param(pe) => match &mut pe.op {
                    ParamOp::Plain | ParamOp::Length => {}
                    ParamOp::Default(w)
                    | ParamOp::Assign(w)
                    | ParamOp::Error(w)
                    | ParamOp::Alternative(w)
                    | ParamOp::RemoveSmallestSuffix(w)
                    | ParamOp::RemoveLargestSuffix(w)
                    | ParamOp::RemoveSmallestPrefix(w)
                    | ParamOp::RemoveLargestPrefix(w)
                    | ParamOp::Bad(w) => word(w),
                },
                _ => {}
            }
        }
        fn redirs(rs: &mut [Redirect]) {
            for r in rs {
                match &mut r.target {
                    RedirTarget::Word(w) => word(w),
                    RedirTarget::HereDoc(hd) => word(&mut hd.borrow_mut().body),
                }
            }
        }
        fn compound(cc: &mut CompoundCommand) {
            match cc {
                CompoundCommand::BraceGroup(l) | CompoundCommand::Subshell(l) => strip_lines(l),
                CompoundCommand::If { conds, else_ } => {
                    for (c, b) in conds {
                        strip_lines(c);
                        strip_lines(b);
                    }
                    if let Some(e) = else_ {
                        strip_lines(e);
                    }
                }
                CompoundCommand::While { cond, body, .. } => {
                    strip_lines(cond);
                    strip_lines(body);
                }
                CompoundCommand::For {
                    words: ws,
                    body,
                    lineno,
                    ..
                } => {
                    *lineno = 0;
                    if let Some(ws) = ws {
                        words(ws);
                    }
                    strip_lines(body);
                }
                CompoundCommand::Case { word: w, arms, lineno } => {
                    *lineno = 0;
                    word(w);
                    for a in arms {
                        words(&mut a.patterns);
                        strip_lines(&mut a.body);
                    }
                }
            }
        }
        for cc in list {
            let ao = &mut cc.list;
            for p in std::iter::once(&mut ao.first).chain(ao.rest.iter_mut().map(|(_, p)| p)) {
                for c in &mut p.cmds {
                    match c {
                        Command::Simple(sc) => {
                            sc.lineno = 0;
                            sc.assigns.iter_mut().for_each(|a| word(&mut a.value));
                            words(&mut sc.words);
                            redirs(&mut sc.redirs);
                        }
                        Command::Compound(cc, rs) => {
                            compound(cc);
                            redirs(rs);
                        }
                        Command::FunctionDef { body, .. } => {
                            let b = std::rc::Rc::make_mut(body);
                            compound(&mut b.cmd);
                            redirs(&mut b.redirs);
                        }
                    }
                }
            }
        }
    }

    /// Printing `src` gives text that parses to the same tree.
    fn round_trip(src: &str) -> String {
        let printed = print(src.as_bytes(), &AliasMap::default());
        let mut a = parse(src.as_bytes());
        let mut b = parse(printed.as_bytes());
        strip_lines(&mut a);
        strip_lines(&mut b);
        assert_eq!(a, b, "\nsource:\n{src}\nprinted:\n{printed}");
        printed
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
        round_trip(r#"f() { echo ${x//a/b} ${x:foo} ${}; }"#);
        round_trip(r#"f() { echo $((1 + $x * (2 - y))) $(( $(echo 1) )) `echo a` "`echo \"b\"`"; }"#);
        round_trip("f() { x=$(a; b) y=$( (sub) ) z=$(case a in a) :;; esac); }");
        round_trip("f() { echo $(if a; then b; fi) \"$(for i in 1; do :; done)\"; }");
        round_trip("f() { ! a | b && c || { d; } & }");
        round_trip("f() { g() { echo in g; }; exec 3<>f 4<&3 5>&- >|c >>d <e; }");
    }

    #[test]
    fn heredocs() {
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
