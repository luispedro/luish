//! `vared`: zsh's, edits the value of a variable with the line editor.

use crate::input::Line;
use crate::interactive::ValueEdit;
use crate::lexer::is_valid_name;
use crate::options::Opt;
use crate::prompt::Prompt;
use crate::shell::{ExecResult, Flow, Shell};
use crate::vars::{Item, Special, Subscript, Value};

/// The options without an argument, and those with one.
const FLAGS: &[u8] = b"Aacegh";
const WITH_ARG: &[u8] = b"fiMmprt";

/// The type of a variable.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Scalar,
    Array,
    Assoc,
}

/// The options given: the flags, and the options with an argument.
#[derive(Default)]
struct Opts {
    flags: Vec<u8>,
    args: Vec<(u8, Vec<u8>)>,
}

impl Opts {
    fn has(&self, c: u8) -> bool {
        self.flags.contains(&c)
    }

    fn arg(&self, c: u8) -> Option<&[u8]> {
        self.args.iter().rev().find(|a| a.0 == c).map(|a| &a.1[..])
    }
}

/// Parses the options as zsh does: letters can be grouped, an option's
/// argument is the rest of its word or the next word, and `-` or `--` ends
/// the options. Returns the options and the operands, or the status after
/// an error.
fn options<'a>(sh: &Shell, name: &[u8], args: &'a [Vec<u8>]) -> Result<(Opts, &'a [Vec<u8>]), i32> {
    let mut o = Opts::default();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if a.len() < 2 || a[0] != b'-' {
            if a == b"-" {
                i += 1;
            }
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        let mut j = 1;
        while j < a.len() {
            let c = a[j];
            j += 1;
            if WITH_ARG.contains(&c) {
                let value = if j < a.len() {
                    a[j..].to_vec()
                } else if let Some(v) = args.get(i) {
                    i += 1;
                    v.clone()
                } else {
                    sh.berr(name, format!("argument expected: -{}", c as char));
                    return Err(1);
                };
                o.args.push((c, value));
                break;
            }
            if !FLAGS.contains(&c) {
                sh.berr(name, format!("bad option: -{}", c as char));
                return Err(1);
            }
            o.flags.push(c);
        }
    }
    Ok((o, &args[i..]))
}

pub fn vared(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let name = &argv[0];
    let (o, operands) = match options(sh, name, &argv[1..]) {
        Ok(r) => r,
        Err(status) => return Ok(status),
    };
    if o.has(b'a') && o.has(b'A') {
        sh.berr(name, "specify only one of -a and -A");
        return Ok(1);
    }
    let target = match operands {
        [] => {
            sh.berr(name, "not enough arguments");
            return Ok(1);
        }
        [t] => t,
        _ => {
            sh.berr(name, "too many arguments");
            return Ok(1);
        }
    };
    // luish has no widgets of the user's own, and edits on standard input.
    for &c in b"ift" {
        if o.arg(c).is_some() {
            sh.berr(name, format!("-{}: not supported", c as char));
            return Ok(1);
        }
    }
    let vi = match o.arg(b'M') {
        None | Some(b"main") => sh.opt(Opt::Vi),
        Some(b"emacs") => false,
        Some(b"viins") => true,
        Some(m) => return Ok(no_keymap(sh, name, m)),
    };
    if let Some(m) = o.arg(b'm')
        && m != b"vicmd"
    {
        return Ok(no_keymap(sh, name, m));
    }
    // `vared 'a[i]'` edits an element; `a[@]` and `a[*]` are the array.
    let (var, sub) = match target.split_last() {
        Some((b']', rest)) if let Some(open) = rest.iter().position(|&c| c == b'[') => {
            let sub = &rest[open + 1..];
            (&rest[..open], (sub != b"@" && sub != b"*").then_some(sub))
        }
        _ => (&target[..], None),
    };
    if !is_valid_name(var) {
        sh.berr(name, format!("{}: bad variable name", String::from_utf8_lossy(target)));
        return Ok(1);
    }
    if sh.vars.var(var).is_some_and(|v| v.readonly) {
        sh.berr(name, format!("read-only variable: {}", String::from_utf8_lossy(var)));
        return Ok(1);
    }
    let create = o.has(b'c');
    let current = current(sh, var);
    if current.is_none() && !create {
        sh.berr(name, format!("no such variable: {}", String::from_utf8_lossy(target)));
        return Ok(1);
    }
    let ifs = sh.get_var(b"IFS").unwrap_or_else(|| b" \t\n".to_vec());
    // An element is edited as a string; with `-c`, the variable becomes
    // of the type asked for (a string without `-a` or `-A`).
    let (text, kind) = match (sub, &current) {
        (Some(sub), _) => {
            let sub = match sh.vars.is_assoc(var) {
                true => Subscript::Key(sub.to_vec()),
                false => match crate::expand::arith::eval(sh, sub) {
                    Ok(i) => Subscript::Index(i),
                    Err(msg) => {
                        sh.berr(name, msg);
                        return Ok(1);
                    }
                },
            };
            let text = element(sh, var, &sub);
            return edit(sh, name, &o, vi, text).and_then(|line| match line {
                Some(text) => sh.set_element(var, &sub, text, false).map(|()| 0),
                None => Ok(1),
            });
        }
        (None, None) => (Vec::new(), None),
        (None, Some((Kind::Scalar, v))) => (v[0].clone(), Some(Kind::Scalar)),
        (None, Some((k, v))) => (join(v, &ifs), Some(*k)),
    };
    let want = if !create {
        for &c in b"aA" {
            if o.has(c) {
                sh.berr(name, format!("-{} ignored", c as char));
            }
        }
        kind.unwrap_or(Kind::Scalar)
    } else if o.has(b'a') {
        Kind::Array
    } else if o.has(b'A') {
        Kind::Assoc
    } else {
        Kind::Scalar
    };
    let Some(text) = edit(sh, name, &o, vi, text)? else {
        return Ok(1);
    };
    let fields = (want != Kind::Scalar).then(|| split(&text, &ifs));
    if want == Kind::Assoc && fields.as_ref().is_some_and(|f| f.len() % 2 != 0) {
        sh.berr(name, "bad set of key/value pairs for associative array");
        return Ok(1);
    }
    // Another type replaces the variable.
    if kind.is_some_and(|k| k != want) {
        let _ = sh.unset_var(var);
    }
    match (want, fields) {
        (Kind::Assoc, Some(f)) => {
            if kind != Some(Kind::Assoc) {
                sh.set_var_value(var, Value::Assoc(Box::default()))?;
            }
            sh.assign_items(var, items(f), false)?;
        }
        (_, Some(f)) => sh.assign_items(var, items(f), false)?,
        (_, None) => sh.set_var(var, text)?,
    }
    Ok(0)
}

fn no_keymap(sh: &Shell, name: &[u8], map: &[u8]) -> i32 {
    sh.berr(name, format!("no such keymap `{}'", String::from_utf8_lossy(map)));
    1
}

/// The type of a variable and its value (keys and values in turn for an
/// associative array), or None if it is unset.
fn current(sh: &Shell, var: &[u8]) -> Option<(Kind, Vec<Vec<u8>>)> {
    match sh.vars.get_value(var) {
        Some(Value::Str(s)) => Some((Kind::Scalar, vec![s.clone()])),
        Some(Value::Array(a)) => Some((Kind::Array, a.to_vec())),
        Some(Value::Assoc(h)) => {
            let pairs = h.keys().iter().zip(h.values());
            Some((Kind::Assoc, pairs.flat_map(|(k, v)| [k.clone(), v.clone()]).collect()))
        }
        None if sh.vars.special(var).is_some_and(Special::is_array) => Some((Kind::Array, sh.special_elements(var)?)),
        None => sh.get_var(var).map(|v| (Kind::Scalar, vec![v])),
    }
}

/// The value of an element, empty if there is none.
fn element(sh: &Shell, var: &[u8], sub: &Subscript) -> Vec<u8> {
    match (sh.vars.get_value(var), sub) {
        (Some(Value::Assoc(h)), Subscript::Key(k)) => h.get(k).cloned(),
        (v, &Subscript::Index(i)) => {
            let elements = match v {
                Some(v) => v.elements().to_vec(),
                None => sh.special_elements(var).unwrap_or_default(),
            };
            let i = if i < 0 { i + elements.len() as i64 } else { i };
            usize::try_from(i).ok().and_then(|i| elements.get(i).cloned())
        }
        _ => None,
    }
    .unwrap_or_default()
}

/// Edits `text`. None if the editing was given up: Ctrl-D on an empty line
/// (with `-e`), Ctrl-C (which also interrupts the shell, as at the prompt),
/// or an error, which was reported.
fn edit(sh: &mut Shell, name: &[u8], o: &Opts, vi: bool, text: Vec<u8>) -> Result<Option<Vec<u8>>, Flow> {
    let Ok(text) = String::from_utf8(text) else {
        sh.berr(name, "the value isn't valid UTF-8");
        return Ok(None);
    };
    let prompt = |sh: &Shell, c| o.arg(c).map(|p| crate::prompt::expand(sh, p));
    let v = ValueEdit {
        prompt: prompt(sh, b'p').unwrap_or_else(|| Prompt::plain(Vec::new())),
        right: prompt(sh, b'r').filter(|p| !p.text.is_empty()),
        text,
        history: o.has(b'h'),
        eof: o.has(b'e'),
        vi,
    };
    match crate::interactive::edit_value(sh, v) {
        Ok(Line::Text(mut t)) => {
            if t.last() == Some(&b'\n') {
                t.pop();
            }
            Ok(Some(t))
        }
        Ok(Line::Eof) => Ok(None),
        // As at the prompt (where the editor has started a new line), or
        // as SIGINT would: a subshell gets it, and a trap runs now.
        Ok(Line::Interrupted) if sh.interactive && sh.traps[libc::SIGINT as usize].is_none() => {
            Err(Flow::Error(128 + libc::SIGINT))
        }
        Ok(Line::Interrupted) => {
            // SAFETY: raise has no preconditions.
            unsafe { libc::raise(libc::SIGINT) };
            sh.run_pending_traps()?;
            Ok(None)
        }
        Err(msg) => {
            sh.berr(name, msg);
            Ok(None)
        }
    }
}

/// Joins the elements of an array for editing, with the first character of
/// `IFS` between them (none if it is empty), and a backslash before each
/// character of `IFS` and each backslash in them, as zsh does.
fn join(elements: &[Vec<u8>], ifs: &[u8]) -> Vec<u8> {
    let sep = match ifs.first() {
        Some(&c) if c >= 0xc0 => &ifs[..(c.leading_ones() as usize).min(ifs.len())],
        _ => &ifs[..ifs.len().min(1)],
    };
    let mut out = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(sep);
        }
        for &c in e {
            if c == b'\\' || ifs.contains(&c) {
                out.push(b'\\');
            }
            out.push(c);
        }
    }
    out
}

/// Splits the edited text into elements, as zsh does: at the characters
/// of `IFS` (where blanks around one count as one), unless a backslash
/// quotes them (it also quotes a backslash; before anything else it stays).
/// Unlike field splitting, a separator other than a blank at the end makes
/// an empty element.
fn split(text: &[u8], ifs: &[u8]) -> Vec<Vec<u8>> {
    let is_sep = |c: u8| ifs.contains(&c);
    let is_blank = |c: u8| is_sep(c) && matches!(c, b' ' | b'\t' | b'\n');
    let skip_blanks = |mut i: usize| {
        while i < text.len() && is_blank(text[i]) {
            i += 1;
        }
        i
    };
    let mut out = Vec::new();
    let mut i = skip_blanks(0);
    if i < text.len() && is_sep(text[i]) {
        out.push(Vec::new());
    }
    while i < text.len() {
        if is_sep(text[i]) {
            i = skip_blanks(i + 1);
        }
        let mut field = Vec::new();
        while i < text.len() && !is_sep(text[i]) {
            match text.get(i + 1) {
                Some(&c) if text[i] == b'\\' && (c == b'\\' || is_sep(c)) => {
                    field.push(c);
                    i += 2;
                }
                _ => {
                    field.push(text[i]);
                    i += 1;
                }
            }
        }
        out.push(field);
        i = skip_blanks(i);
    }
    out
}

fn items(fields: Vec<Vec<u8>>) -> Vec<Item> {
    fields.into_iter().map(|value| Item { key: None, value }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<Vec<u8>> {
        items.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    #[test]
    fn join_quotes() {
        let ifs = b" \t\n";
        assert_eq!(join(&v(&["x", "y z", "w\\v", "", "q"]), ifs), b"x y\\ z w\\\\v  q");
        assert_eq!(join(&v(&["a:b", "c d"]), b":"), b"a\\:b:c d");
        assert_eq!(join(&v(&["a", "b"]), b""), b"ab");
        assert_eq!(join(&v(&["a", "b"]), "·:".as_bytes()), "a·b".as_bytes());
    }

    #[test]
    fn split_as_zsh() {
        let ifs = b" \t\n";
        assert_eq!(split(b"x y\\ z w\\\\v  q", ifs), v(&["x", "y z", "w\\v", "q"]));
        assert_eq!(split(b" a  b  ", ifs), v(&["a", "b"]));
        assert_eq!(split(b"", ifs), v(&[]));
        assert_eq!(split(b"   ", ifs), v(&[]));
        assert_eq!(split(b"a\\b \\", ifs), v(&["a\\b", "\\"]));
        let ifs = b": ";
        assert_eq!(split(b"X\\ Y:a\\\\b\\:c::", ifs), v(&["X Y", "a\\b:c", "", ""]));
        assert_eq!(split(b":a", ifs), v(&["", "a"]));
        assert_eq!(split(b"a : b", ifs), v(&["a", "b"]));
        assert_eq!(split(b"a::b", ifs), v(&["a", "", "b"]));
    }

    #[test]
    fn round_trip() {
        let ifs = b" \t\n:";
        let a = v(&["a b", "c:d", "e\\", "\\f"]);
        assert_eq!(split(&join(&a, ifs), ifs), a);
    }
}
