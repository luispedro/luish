//! The text of a command as shown by `jobs`, `fg` and job notifications.
//!
//! This follows dash's `cmdtxt`: assignments are dropped, `$x` is shown as
//! `${x}`, single quotes become double quotes, command substitutions become
//! `$(...)`, brace groups lose their braces, and a `case` arm shows only its
//! first pattern.

use crate::ast::*;

pub fn command(cmd: &Command) -> String {
    let mut out = Vec::new();
    push_command(&mut out, cmd);
    String::from_utf8_lossy(&out).into_owned()
}

pub fn simple(sc: &SimpleCommand) -> String {
    let mut out = Vec::new();
    push_simple(&mut out, sc);
    String::from_utf8_lossy(&out).into_owned()
}

pub fn compound(cc: &CompoundCommand) -> String {
    let mut out = Vec::new();
    push_compound(&mut out, cc);
    String::from_utf8_lossy(&out).into_owned()
}

pub fn and_or(ao: &AndOrList) -> String {
    let mut out = Vec::new();
    push_and_or(&mut out, ao);
    String::from_utf8_lossy(&out).into_owned()
}

fn push_list(out: &mut Vec<u8>, list: &List) {
    for (i, cc) in list.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b"; ");
        }
        push_and_or(out, &cc.list);
    }
}

fn push_and_or(out: &mut Vec<u8>, ao: &AndOrList) {
    push_pipeline(out, &ao.first);
    for (kind, p) in &ao.rest {
        out.extend_from_slice(match kind {
            AndOr::And => b" && ",
            AndOr::Or => b" || ",
        });
        push_pipeline(out, p);
    }
}

fn push_pipeline(out: &mut Vec<u8>, p: &Pipeline) {
    if p.negated {
        out.push(b'!');
    }
    for (i, c) in p.cmds.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(b" | ");
        }
        push_command(out, c);
    }
}

fn push_command(out: &mut Vec<u8>, cmd: &Command) {
    match cmd {
        Command::Simple(sc) => push_simple(out, sc),
        // dash does not show the redirections of compound commands.
        Command::Compound(cc, _) => push_compound(out, cc),
        Command::FunctionDef { name, .. } => {
            out.extend_from_slice(name);
            out.extend_from_slice(b"() { ... }");
        }
    }
}

fn push_simple(out: &mut Vec<u8>, sc: &SimpleCommand) {
    for (i, w) in sc.words.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        push_word(out, w);
    }
    push_redirs(out, &sc.redirs);
}

fn push_compound(out: &mut Vec<u8>, cc: &CompoundCommand) {
    match cc {
        CompoundCommand::BraceGroup(list) => push_list(out, list),
        CompoundCommand::Subshell(list) => {
            out.push(b'(');
            push_list(out, list);
            out.push(b')');
        }
        CompoundCommand::If { conds, else_ } => {
            // `elif` is shown as a nested `if`, as dash parses it.
            for (i, (cond, body)) in conds.iter().enumerate() {
                out.extend_from_slice(if i == 0 { b"if " } else { b"; else if " });
                push_list(out, cond);
                out.extend_from_slice(b"; then ");
                push_list(out, body);
            }
            if let Some(e) = else_ {
                out.extend_from_slice(b"; else ");
                push_list(out, e);
            }
            for _ in conds {
                out.extend_from_slice(b"; fi");
            }
        }
        CompoundCommand::While { cond, body, until } => {
            out.extend_from_slice(if *until { b"until " } else { b"while " });
            push_list(out, cond);
            out.extend_from_slice(b"; do ");
            push_list(out, body);
            out.extend_from_slice(b"; done");
        }
        CompoundCommand::For { var, words, body, .. } => {
            out.extend_from_slice(b"for ");
            out.extend_from_slice(var);
            out.extend_from_slice(b" in ");
            match words {
                Some(ws) => {
                    for (i, w) in ws.iter().enumerate() {
                        if i > 0 {
                            out.push(b' ');
                        }
                        push_word(out, w);
                    }
                }
                None => out.extend_from_slice(b"\"${@}\""),
            }
            out.extend_from_slice(b"; do ");
            push_list(out, body);
            out.extend_from_slice(b"; done");
        }
        CompoundCommand::Case { word, arms, .. } => {
            out.extend_from_slice(b"case ");
            push_word(out, word);
            out.extend_from_slice(b" in ");
            for arm in arms {
                if let Some(p) = arm.patterns.first() {
                    push_word(out, p);
                }
                out.extend_from_slice(b") ");
                push_list(out, &arm.body);
                out.extend_from_slice(b";; ");
            }
            out.extend_from_slice(b"esac");
        }
    }
}

fn push_redirs(out: &mut Vec<u8>, redirs: &[Redirect]) {
    for r in redirs {
        out.push(b' ');
        if r.kind == RedirKind::HereDoc {
            out.extend_from_slice(b"<<...");
            continue;
        }
        out.extend_from_slice(r.fd.unwrap_or(r.kind.default_fd()).to_string().as_bytes());
        out.extend_from_slice(match r.kind {
            RedirKind::In => b"<",
            RedirKind::Out => b">",
            RedirKind::Append => b">>",
            RedirKind::Clobber => b">|",
            RedirKind::ReadWrite => b"<>",
            RedirKind::DupIn => b"<&",
            RedirKind::DupOut => b">&",
            RedirKind::HereDoc => unreachable!(),
        });
        if let RedirTarget::Word(w) = &r.target {
            push_word(out, w);
        }
    }
}

fn push_word(out: &mut Vec<u8>, w: &Word) {
    for p in &w.0 {
        push_part(out, p);
    }
}

/// Literal text: the characters that dash stores unescaped but that can
/// only appear quoted are shown with a backslash.
fn push_literal(out: &mut Vec<u8>, s: &[u8]) {
    for &c in s {
        if matches!(c, b'\'' | b'\\' | b'"' | b'$') {
            out.push(b'\\');
        }
        out.push(c);
    }
}

fn push_part(out: &mut Vec<u8>, p: &WordPart) {
    match p {
        WordPart::Literal(s) => push_literal(out, s),
        WordPart::SingleQuoted(s) => {
            out.push(b'"');
            push_literal(out, s);
            out.push(b'"');
        }
        WordPart::DoubleQuoted(parts) => {
            out.push(b'"');
            for p in parts {
                push_part(out, p);
            }
            out.push(b'"');
        }
        WordPart::Escaped(c) => out.push(*c),
        WordPart::Tilde(user) => {
            out.push(b'~');
            out.extend_from_slice(user);
        }
        WordPart::Param(pe) => push_param(out, pe),
        WordPart::CmdSubst(_) => out.extend_from_slice(b"$(...)"),
        WordPart::Arith(w) => {
            out.extend_from_slice(b"$((");
            push_word(out, w);
            out.extend_from_slice(b"))");
        }
        WordPart::GlobQual(q) => {
            out.push(b'(');
            out.extend_from_slice(q);
            out.push(b')');
        }
    }
}

fn push_param(out: &mut Vec<u8>, pe: &ParamExp) {
    out.extend_from_slice(if pe.op == ParamOp::Length { b"${#" } else { b"${" });
    match &pe.name {
        ParamName::Var(n) => out.extend_from_slice(n),
        ParamName::Positional(n) => out.extend_from_slice(n.to_string().as_bytes()),
        ParamName::Special(c) => out.push(*c),
    }
    let (op, w): (&[u8], _) = match &pe.op {
        ParamOp::Plain | ParamOp::Length => (b"", None),
        ParamOp::Default(w) => (b"-", Some(w)),
        ParamOp::Assign(w) => (b"=", Some(w)),
        ParamOp::Error(w) => (b"?", Some(w)),
        ParamOp::Alternative(w) => (b"+", Some(w)),
        ParamOp::RemoveSmallestSuffix(w) => (b"%", Some(w)),
        ParamOp::RemoveLargestSuffix(w) => (b"%%", Some(w)),
        ParamOp::RemoveSmallestPrefix(w) => (b"#", Some(w)),
        ParamOp::RemoveLargestPrefix(w) => (b"##", Some(w)),
        ParamOp::Bad(w) => (b"", Some(w)),
    };
    if pe.colon {
        out.push(b':');
    }
    out.extend_from_slice(op);
    if let Some(w) = w {
        push_word(out, w);
    }
    out.push(b'}');
}

#[cfg(test)]
mod tests {
    use crate::lexer::Parser;

    fn text(src: &str) -> String {
        let mut p = Parser::new(src.as_bytes().to_vec(), 1, true);
        let list = p.parse_all().unwrap();
        super::and_or(&list[0].list)
    }

    /// Expected values are what dash 0.5.12 shows in `jobs`.
    #[test]
    fn matches_dash() {
        assert_eq!(
            text("x=1 sleep 11 2>&1 >/dev/null </dev/null"),
            "sleep 11 2>&1 1>/dev/null 0</dev/null"
        );
        assert_eq!(text("sleep 12 | cat | sleep 13"), "sleep 12 | cat | sleep 13");
        assert_eq!(text("{ echo a; sleep 14; } >/dev/null"), "echo a; sleep 14");
        assert_eq!(
            text("if true; then sleep 15; elif false; then :; else :; fi"),
            "if true; then sleep 15; else if false; then :; else :; fi; fi"
        );
        assert_eq!(text("(sleep 16; echo \"$x\")"), "(sleep 16; echo \"${x}\")");
        assert_eq!(text("until false; do sleep 17; done"), "until false; do sleep 17; done");
        assert_eq!(
            text("for i in a \"$x\"; do sleep 18; done"),
            "for i in a \"${x}\"; do sleep 18; done"
        );
        assert_eq!(text("for i; do :; done"), "for i in \"${@}\"; do :; done");
        assert_eq!(
            text("case $x in a|b) sleep 19 ;; *) echo ;; esac"),
            "case ${x} in a) sleep 19;; *) echo;; esac"
        );
        assert_eq!(
            text(
                "echo ${x:-y} ${#x} ${x#p} \"${x%%q}\" $(echo hi) `echo z` $((1+x)) \"q $x\" 'r s' \\t a\\ b 'x\"y' \"a\\$b\" ~/f > /dev/null"
            ),
            "echo ${x:-y} ${#x} ${x#p} \"${x%%q}\" $(...) $(...) $((1+x)) \"q ${x}\" \"r s\" t a b \"x\\\"y\" \"a$b\" ~/f 1>/dev/null"
        );
        assert_eq!(text("! true && false || true"), "!true && false || true");
        assert_eq!(text("sleep 20 <<X\nbody\nX\n"), "sleep 20 <<...");
        assert_eq!(text("g() { sleep 1; }"), "g() { ... }");
    }
}
