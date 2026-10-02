//! The fuzz targets, one function each (called by `fuzz_targets/NAME.rs`).
//! Besides not panicking, each checks what it can of its result.

use std::cell::Cell;
use std::rc::Rc;

use crate::ast::{Command, List};
use crate::expand::pattern::{self, Pattern, Trim};
use crate::expand::split::XChar;
use crate::interactive::{bang, highlight, history::ShellHistory};
use crate::lexer::{AliasMap, Parser, no_aliases};
use crate::shell::Shell;
use crate::{ast, cmdtext, stack, unparse};

/// Aliases that exercise the lexer's splicing: one ending in a blank (so
/// the next word is expanded too), recursive ones, ones that open or close
/// a construct, a global one and a suffix one.
fn aliases() -> Rc<AliasMap> {
    let mut a = AliasMap::default();
    for (name, value) in [
        ("a", "echo "),
        ("b", "b c "),
        ("c", "a b"),
        ("i", "if true; then"),
        ("o", "("),
        ("q", "'"),
        ("x", "x=1 "),
        ("l", "ls -l <<E\n"),
    ] {
        a.insert(name.into(), value.into(), false);
    }
    a.insert(b"G".into(), b"| wc -l".into(), true);
    a.insert_suffix(b"txt".into(), b"cat".into());
    Rc::new(a)
}

/// Parses all of `src`, and the job text of each command, as `cmdtext`
/// writes it. Stops at the first error.
fn parse_with(src: &[u8], aliases: &Rc<AliasMap>, source_eof: bool, bareglobqual: bool) -> Option<List> {
    let mut p = Parser::new(src.to_vec(), 1, source_eof);
    p.bareglobqual = bareglobqual;
    let mut all = Vec::new();
    loop {
        match p.parse_next(aliases) {
            Ok(Some(list)) => {
                // What the interactive loop and `eval` use to find the rest.
                assert!(p.consumed() <= src.len(), "consumed {} of {}", p.consumed(), src.len());
                for cc in &list {
                    cmdtext::and_or(&cc.list);
                }
                all.extend(list);
            }
            Ok(None) => return Some(all),
            Err(e) => {
                assert!(
                    !e.incomplete || !source_eof,
                    "incomplete at the end of the source: {}",
                    e.msg
                );
                return None;
            }
        }
    }
}

/// The parser and lexer: on the whole input (as for `-c` and scripts), as
/// the interactive loop parses it (where it may be incomplete), and with
/// aliases and glob qualifiers.
pub fn parse(data: &[u8]) {
    stack::init();
    parse_with(data, &no_aliases(), true, false);
    parse_with(data, &no_aliases(), false, false);
    parse_with(data, &aliases(), true, true);
}

/// The functions defined in `list`, at its top level.
fn functions(list: &List) -> Vec<(Vec<u8>, Rc<ast::FunctionBody>)> {
    let mut fs = Vec::new();
    for cc in list {
        let ao = &cc.list;
        for p in std::iter::once(&ao.first).chain(ao.rest.iter().map(|(_, p)| p)) {
            for c in &p.cmds {
                if let Command::FunctionDef { names, body } = c {
                    fs.push((names[0].clone(), body.clone()));
                }
            }
        }
    }
    fs
}

/// `unparse.rs`, which writes functions back as source text (`typeset -f`,
/// the saved state): the text must parse back to the same tree, apart from
/// line numbers, which the saved state records separately and must fit
/// that tree. The input is made the body of a function.
pub fn unparse(data: &[u8]) {
    stack::init();
    let src = [&b"f() {\n"[..], data, b"\n}\n"].concat();
    let Some(list) = parse_with(&src, &no_aliases(), true, false) else {
        return;
    };
    let aliases = AliasMap::default();
    for (name, body) in functions(&list) {
        let (text, globqual) = unparse::function(&name, &body, &aliases);
        let shown = String::from_utf8_lossy(&text);
        let Some(back) = parse_with(&text, &no_aliases(), true, globqual) else {
            panic!("printed function doesn't parse:\n{shown}");
        };
        let back = functions(&back);
        assert_eq!(
            back.len(),
            1,
            "printed function parses to {} functions:\n{shown}",
            back.len()
        );
        let mut back_body = (*back[0].1).clone();
        assert!(
            unparse::set_body_lines(&mut back_body, &unparse::body_lines(&body)),
            "the line numbers don't fit the printed function:\n{shown}"
        );
        let mut a: List = vec![function_command(&name, &body)];
        let mut b: List = vec![function_command(&back[0].0, &back[0].1)];
        unparse::strip_lines(&mut a);
        unparse::strip_lines(&mut b);
        assert!(
            a == b,
            "printed function parses to a different tree:\n{shown}\n{a:#?}\n{b:#?}"
        );
    }
}

fn function_command(name: &[u8], body: &Rc<ast::FunctionBody>) -> ast::CompleteCommand {
    ast::CompleteCommand {
        list: ast::AndOrList {
            first: ast::Pipeline {
                negated: false,
                cmds: vec![Command::FunctionDef {
                    names: vec![name.to_vec()],
                    body: Rc::new((**body).clone()),
                }],
            },
            rest: Vec::new(),
        },
        async_: false,
    }
}

/// `$((...))`, on text after its expansions (which is what it gets).
pub fn arith(data: &[u8]) {
    stack::init();
    let mut sh = Shell::new();
    let _ = sh.vars.set(b"v", b"7".to_vec());
    let _ = sh.vars.set(b"w", b"0x7fffffffffffffff".to_vec());
    let _ = sh.vars.set(b"bad", b"1+".to_vec());
    let _ = crate::expand::arith::eval(&mut sh, data);
}

/// The pattern matcher: `${x#pat}` and friends and `${x/pat/rep}` only try
/// the prefixes and suffixes that the pattern could match, which must give
/// what trying them all gives. The input is the pattern (with `\` quoting
/// the next byte), the string and the replacement, separated by NULs.
pub fn pattern(data: &[u8]) {
    let mut parts = data.splitn(3, |&b| b == 0);
    let (pat, s, rep) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    // The checks are quadratic in the string.
    if s.len() > 200 {
        return;
    }
    let mut xp = Vec::new();
    let mut it = pat.iter();
    while let Some(&b) = it.next() {
        xp.push(match b {
            b'\\' => XChar {
                b: it.next().copied().unwrap_or(b'\\'),
                quoted: true,
            },
            _ => XChar { b, quoted: false },
        });
    }
    pattern::has_meta(&xp);
    let p = Pattern::new(&xp);
    let m = |k: std::ops::Range<usize>| p.matches(&s[k]);
    let n = s.len();

    let smallest_prefix = (0..=n).find(|&k| m(0..k));
    let largest_prefix = (0..=n).rev().find(|&k| m(0..k));
    let smallest_suffix = (0..=n).find(|&k| m(n - k..n));
    let largest_suffix = (0..=n).rev().find(|&k| m(n - k..n));
    let kept = |how| pattern::trim(s, &xp, how).len();
    assert_eq!(kept(Trim::SmallestPrefix), n - smallest_prefix.unwrap_or(0), "#");
    assert_eq!(kept(Trim::LargestPrefix), n - largest_prefix.unwrap_or(0), "##");
    assert_eq!(kept(Trim::SmallestSuffix), n - smallest_suffix.unwrap_or(0), "%");
    assert_eq!(kept(Trim::LargestSuffix), n - largest_suffix.unwrap_or(0), "%%");

    let replace = |how| pattern::replace(s, &xp, how, rep);
    let expected = match largest_prefix {
        Some(k) => [rep, &s[k..]].concat(),
        None => s.to_vec(),
    };
    assert_eq!(replace(ast::Replace::Prefix), expected, "/#");
    let expected = match largest_suffix {
        Some(k) => [&s[..n - k], rep].concat(),
        None => s.to_vec(),
    };
    assert_eq!(replace(ast::Replace::Suffix), expected, "/%");
    for all in [false, true] {
        let expected = if xp.is_empty() {
            s.to_vec()
        } else if n == 0 {
            if m(0..0) { rep.to_vec() } else { Vec::new() }
        } else {
            // The longest non-empty match at each position in turn.
            let mut out = Vec::new();
            let mut i = 0;
            while i < n {
                match (1..=n - i).rev().find(|&k| m(i..i + k)) {
                    Some(k) => {
                        out.extend_from_slice(rep);
                        i += k;
                        if !all {
                            break;
                        }
                    }
                    None => {
                        out.push(s[i]);
                        i += 1;
                    }
                }
            }
            out.extend_from_slice(&s[i..]);
            out
        };
        let how = if all { ast::Replace::All } else { ast::Replace::First };
        assert_eq!(replace(how), expected, "{}", if all { "//" } else { "/" });
    }
}

/// The syntax highlighter, which sees every line as it is typed (so any
/// prefix of a command): one class per byte, and the line unchanged once
/// the colours are taken out.
pub fn highlight(data: &[u8]) {
    stack::init();
    let command = |w: &[u8]| match w.len() % 3 {
        0 => highlight::CommandKind::Builtin,
        1 => highlight::CommandKind::Function,
        _ => highlight::CommandKind::Unknown,
    };
    let var = |w: &[u8]| (w.first() == Some(&b'H')).then_some(highlight::VarKind::Plain);
    let path = |p: &[u8], at_cursor: bool| match p.len() % 3 {
        0 => highlight::PATH,
        1 if at_cursor => highlight::PATH_PREFIX,
        _ => 0,
    };
    let facts = highlight::Facts {
        command: &command,
        var: &var,
        braces: true,
        glob: true,
        path: Some(&path),
        home: Some(b"/home"),
    };
    let colors = highlight::Colors::default();
    let error = highlight::syntax_error(data, &aliases(), false);
    for cursor in [None, Some(data.len()), Some(data.len() / 2)] {
        let mut cls = highlight::classify(data, cursor, &facts);
        assert_eq!(cls.len(), data.len());
        if let Some(start) = error.clone().and_then(|e| highlight::error_start(data, e, cursor)) {
            for c in &mut cls[start..] {
                c.mods |= highlight::ERROR;
            }
        }
        let out = highlight::render(data, &cls, &colors);
        // (A line with an escape of its own could be taken for one.)
        if !data.contains(&0x1b) {
            assert_eq!(strip_sgr(&out), data);
        }
    }
}

/// `out` without the `ESC [ ... m` sequences that `render` adds.
fn strip_sgr(out: &[u8]) -> Vec<u8> {
    let mut plain = Vec::with_capacity(out.len());
    let mut i = 0;
    while i < out.len() {
        if out[i..].starts_with(b"\x1b[")
            && let Some(m) = out[i + 2..].iter().position(|&b| !(b.is_ascii_digit() || b == b';'))
            && out[i + 2 + m] == b'm'
        {
            i += m + 3;
        } else {
            plain.push(out[i]);
            i += 1;
        }
    }
    plain
}

/// History expansion (`!!`, `!$`, `^old^new`...). The input is lines: all
/// but the last are the history, and the last is the line to expand (twice,
/// for what the first expansion leaves for the next in `Memory`).
pub fn bang(data: &[u8]) {
    let mut lines: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
    let line = lines.pop().unwrap_or_default();
    let mut h = ShellHistory::default();
    for l in lines {
        h.add_entry(&String::from_utf8_lossy(l));
    }
    let mut mem = bang::Memory::default();
    for _ in 0..2 {
        let _ = bang::expand(&h, &mut mem, b"", line);
    }
}

thread_local! {
    static SHELL: Shell = Shell::new();
}

/// Prompt expansion (`%` sequences in `PS1`).
pub fn prompt(data: &[u8]) {
    stack::init();
    SHELL.with(|sh| crate::prompt::expand(sh, data));
}
