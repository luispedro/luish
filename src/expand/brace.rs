//! Brace expansion (`setopt expand.braces`): `a{b,c}` gives `ab ac`, and
//! `{1..3}` gives `1 2 3`, before the other expansions of a word, as in
//! bash. The braces, commas and `..` are found in the word's unquoted
//! literal text, so quoted ones and those of `${x}` don't count. As in zsh
//! (where expansions come first), the ends and the step of a sequence can
//! be expansions (`{1..$n}`), which are expanded once, here; a sequence of
//! characters can be of any characters (UTF-8 included), not only letters.

use crate::ast::{Parts, Word, WordPart};

/// A piece of a word: a byte of its unquoted literal text, which can be
/// one of the braces' `{`, `,`, `}` and `.`, or another part, kept whole.
#[derive(Debug, Clone)]
enum Atom {
    Byte(u8),
    Part(WordPart),
}

fn is(a: &Atom, c: u8) -> bool {
    matches!(a, Atom::Byte(b) if *b == c)
}

/// Whether brace expansion could change `w`: whether it has an unquoted
/// `{` (also in a tilde prefix, as in `~{,/tmp}`).
pub fn has_brace(w: &Word) -> bool {
    w.0.iter().any(|p| match p {
        WordPart::Literal(s) | WordPart::Tilde(s) => s.contains(&b'{'),
        _ => false,
    })
}

/// The words that `w` expands to, or `None` if it has no braces to expand.
/// `text` expands a word without field splitting (the ends of a sequence).
pub fn expand<E>(w: &Word, text: &mut dyn FnMut(&Word) -> Result<Vec<u8>, E>) -> Result<Option<Vec<Word>>, E> {
    let mut atoms = Vec::new();
    for (i, p) in w.0.iter().enumerate() {
        match p {
            WordPart::Literal(s) => atoms.extend(s.iter().map(|&c| Atom::Byte(c))),
            // The lexer took the braces as part of the tilde prefix, which
            // is found again in each word (`to_word`).
            WordPart::Tilde(user) if i == 0 => {
                atoms.push(Atom::Byte(b'~'));
                atoms.extend(user.iter().map(|&c| Atom::Byte(c)));
            }
            _ => atoms.push(Atom::Part(p.clone())),
        }
    }
    let mut changed = false;
    let words = braces(atoms, text, &mut changed)?;
    Ok(changed.then(|| words.into_iter().map(to_word).collect()))
}

/// Expands the first brace expression of `atoms` (with the text after it),
/// leaving the `{` of those that aren't one (`{a}`, `{x`) as they are.
fn braces<E>(
    mut atoms: Vec<Atom>,
    text: &mut dyn FnMut(&Word) -> Result<Vec<u8>, E>,
    changed: &mut bool,
) -> Result<Vec<Vec<Atom>>, E> {
    let mut i = 0;
    while i < atoms.len() {
        if !is(&atoms[i], b'{') {
            i += 1;
            continue;
        }
        let Some(close) = matching(&atoms, i) else {
            i += 1;
            continue;
        };
        let items = split_commas(&atoms[i + 1..close]);
        let alternatives = if items.len() > 1 {
            let mut alts = Vec::new();
            for item in items {
                alts.extend(braces(item.to_vec(), text, changed)?);
            }
            alts
        } else {
            match sequence(&atoms[i + 1..close], text)? {
                Sequence::Items(items) => items,
                Sequence::Not(None) => {
                    i += 1;
                    continue;
                }
                // Not a sequence, but its expansions have been expanded:
                // their text replaces them.
                Sequence::Not(Some(body)) => {
                    atoms.splice(i + 1..close, body);
                    *changed = true;
                    i += 1;
                    continue;
                }
            }
        };
        *changed = true;
        let tails = braces(atoms[close + 1..].to_vec(), text, changed)?;
        let prefix = &atoms[..i];
        let mut out = Vec::with_capacity(alternatives.len() * tails.len());
        for a in &alternatives {
            for t in &tails {
                out.push([prefix, a, t].concat());
            }
        }
        return Ok(out);
    }
    Ok(vec![atoms])
}

/// The index of the `}` that closes the `{` at `open`.
fn matching(atoms: &[Atom], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, a) in atoms.iter().enumerate().skip(open) {
        if is(a, b'{') {
            depth += 1;
        } else if is(a, b'}') {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
    }
    None
}

/// The text between braces, split at its commas outside nested braces.
fn split_commas(body: &[Atom]) -> Vec<&[Atom]> {
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (j, a) in body.iter().enumerate() {
        if is(a, b'{') {
            depth += 1;
        } else if is(a, b'}') {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && is(a, b',') {
            items.push(&body[start..j]);
            start = j + 1;
        }
    }
    items.push(&body[start..]);
    items
}

enum Sequence {
    Items(Vec<Vec<Atom>>),
    /// Not a sequence, with the text that replaces the body if any of its
    /// expansions were expanded.
    Not(Option<Vec<Atom>>),
}

/// `{x..y}` and `{x..y..step}`: integers, with an optional sign and
/// leading zeros that pad all of them to the same width, or single
/// characters. As in bash, the sign of the step is ignored (the order
/// is that from `x` to `y`) and a step of 0 is 1.
fn sequence<E>(body: &[Atom], text: &mut dyn FnMut(&Word) -> Result<Vec<u8>, E>) -> Result<Sequence, E> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut j = 0;
    while j + 1 < body.len() {
        if is(&body[j], b'.') && is(&body[j + 1], b'.') {
            pieces.push(&body[start..j]);
            j += 2;
            start = j;
        } else {
            j += 1;
        }
    }
    pieces.push(&body[start..]);
    if !(2..=3).contains(&pieces.len()) {
        return Ok(Sequence::Not(None));
    }
    let mut expanded = false;
    let mut values = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        if piece.iter().all(|a| matches!(a, Atom::Byte(_))) {
            values.push(
                piece
                    .iter()
                    .map(|a| if let Atom::Byte(c) = a { *c } else { 0 })
                    .collect(),
            );
        } else {
            expanded = true;
            values.push(text(&to_word(piece.to_vec()))?);
        }
    }
    if let Some(items) = items(&values) {
        let quoted = |s: Vec<u8>| vec![Atom::Part(WordPart::SingleQuoted(s))];
        return Ok(Sequence::Items(items.into_iter().map(quoted).collect()));
    }
    if !expanded {
        return Ok(Sequence::Not(None));
    }
    let mut body = Vec::new();
    for (k, (piece, v)) in pieces.iter().zip(values).enumerate() {
        if k > 0 {
            body.extend([Atom::Byte(b'.'), Atom::Byte(b'.')]);
        }
        if piece.iter().all(|a| matches!(a, Atom::Byte(_))) {
            body.extend_from_slice(piece);
        } else {
            body.push(Atom::Part(WordPart::SingleQuoted(v)));
        }
    }
    Ok(Sequence::Not(Some(body)))
}

/// The words of a sequence from the text of its ends and step.
fn items(values: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
    let step = match values.get(2) {
        Some(s) => integer(s)?.unsigned_abs().max(1),
        None => 1,
    };
    let (x, y) = (&values[0], &values[1]);
    if let (Some(a), Some(b)) = (integer(x), integer(y)) {
        let width = if padded(x) || padded(y) {
            x.len().max(y.len())
        } else {
            0
        };
        return Some(range(a, b, step).map(|n| pad(n, width)).collect());
    }
    let (a, b) = (character(x)?, character(y)?);
    let chars = range(a as i64, b as i64, step).filter_map(|n| char::from_u32(n as u32));
    Some(chars.map(|c| c.to_string().into_bytes()).collect())
}

/// From `a` to `b`, up or down, by `step`.
fn range(a: i64, b: i64, step: u64) -> impl Iterator<Item = i64> {
    let up = a <= b;
    let mut next = Some(a);
    std::iter::from_fn(move || {
        let n = next?;
        if (up && n > b) || (!up && n < b) {
            return None;
        }
        next = if up {
            n.checked_add_unsigned(step)
        } else {
            n.checked_sub_unsigned(step)
        };
        Some(n)
    })
}

fn integer(s: &[u8]) -> Option<i64> {
    let digits = s.strip_prefix(b"-").or_else(|| s.strip_prefix(b"+")).unwrap_or(s);
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
}

/// Whether an integer has a leading zero, which pads the sequence.
fn padded(s: &[u8]) -> bool {
    let digits = s.strip_prefix(b"-").or_else(|| s.strip_prefix(b"+")).unwrap_or(s);
    digits.len() > 1 && digits[0] == b'0'
}

/// `n` with zeros after its sign up to `width` bytes in all.
fn pad(n: i64, width: usize) -> Vec<u8> {
    if n < 0 {
        format!("-{:0w$}", n.unsigned_abs(), w = width.saturating_sub(1)).into_bytes()
    } else {
        format!("{n:0width$}").into_bytes()
    }
}

/// The character that `s` is, if it is a single one (and not a digit,
/// which is an integer).
fn character(s: &[u8]) -> Option<char> {
    let mut chars = std::str::from_utf8(s).ok()?.chars();
    let c = chars.next()?;
    (chars.next().is_none() && !c.is_ascii_digit()).then_some(c)
}

/// The word of a list of atoms, with its tilde prefix if it starts with
/// an unquoted `~`.
fn to_word(atoms: Vec<Atom>) -> Word {
    let mut parts = Parts::new();
    let mut lit = Vec::new();
    for a in atoms {
        match a {
            Atom::Byte(c) => lit.push(c),
            Atom::Part(p) => {
                if !lit.is_empty() {
                    parts.push(WordPart::Literal(std::mem::take(&mut lit)));
                }
                parts.push(p);
            }
        }
    }
    if !lit.is_empty() {
        parts.push(WordPart::Literal(lit));
    }
    crate::lexer::mark_leading_tilde(&mut parts);
    Word(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Word {
        let mut parts = Parts::One(WordPart::Literal(s.as_bytes().to_vec()));
        crate::lexer::mark_leading_tilde(&mut parts);
        Word(parts)
    }

    /// The words of `s` (all literal), as text, with quoted parts in `'`.
    fn exp(s: &str) -> Vec<String> {
        let mut never = |_: &Word| -> Result<Vec<u8>, ()> { unreachable!() };
        let Some(words) = expand(&lit(s), &mut never).unwrap() else {
            return vec![format!("={s}")];
        };
        words
            .iter()
            .map(|w| {
                w.0.iter()
                    .map(|p| match p {
                        WordPart::Literal(s) => String::from_utf8_lossy(s).into_owned(),
                        WordPart::SingleQuoted(s) => format!("'{}'", String::from_utf8_lossy(s)),
                        WordPart::Tilde(u) => format!("<~{}>", String::from_utf8_lossy(u)),
                        p => format!("{p:?}"),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn lists() {
        assert_eq!(exp("f/{a,b}"), ["f/a", "f/b"]);
        assert_eq!(exp("a{,b}"), ["a", "ab"]);
        assert_eq!(exp("{a,b}{1,2}"), ["a1", "a2", "b1", "b2"]);
        assert_eq!(exp("{a,{b,c}}d"), ["ad", "bd", "cd"]);
        assert_eq!(exp("{x{a,b}}"), ["{xa}", "{xb}"]);
        assert_eq!(exp("{a}{b,c}"), ["{a}b", "{a}c"]);
        assert_eq!(exp("{{a,b}"), ["{a", "{b"]);
        assert_eq!(exp("{a,b}}"), ["a}", "b}"]);
        assert_eq!(exp("{a,}"), ["a", ""]);
        assert_eq!(exp("{a..b,c}"), ["a..b", "c"]);
        for s in [
            "{a}",
            "{}",
            "{a,b",
            "a,b}",
            "{aa..cc}",
            "{1..a}",
            "{a..9}",
            "{1.5..3}",
            "{1..3..x}",
            "{1...3}",
        ] {
            assert_eq!(exp(s), [format!("={s}")]);
        }
    }

    #[test]
    fn sequences() {
        assert_eq!(exp("{0..1}"), ["'0'", "'1'"]);
        assert_eq!(exp("x{5..3}y"), ["x'5'y", "x'4'y", "x'3'y"]);
        assert_eq!(exp("{1..10..3}"), ["'1'", "'4'", "'7'", "'10'"]);
        assert_eq!(exp("{1..10..-3}"), exp("{1..10..3}"));
        assert_eq!(exp("{10..1..3}"), ["'10'", "'7'", "'4'", "'1'"]);
        assert_eq!(exp("{1..3..0}"), exp("{1..3}"));
        assert_eq!(exp("{+1..2}"), ["'1'", "'2'"]);
        assert_eq!(exp("{-1..1}"), ["'-1'", "'0'", "'1'"]);
        assert_eq!(exp("{1..010..4}"), ["'001'", "'005'", "'009'"]);
        assert_eq!(exp("{001..-2}"), ["'001'", "'000'", "'-01'", "'-02'"]);
        assert_eq!(exp("{-05..5..5}"), ["'-05'", "'000'", "'005'"]);
        assert_eq!(exp("{a..e..2}"), ["'a'", "'c'", "'e'"]);
        assert_eq!(exp("{Z..a}"), ["'Z'", "'['", "'\\'", "']'", "'^'", "'_'", "'`'", "'a'"]);
        assert_eq!(exp("{α..γ}"), ["'α'", "'β'", "'γ'"]);
        assert_eq!(exp("{x..x}"), ["'x'"]);
        assert_eq!(
            exp("{9223372036854775806..9223372036854775807..5}"),
            ["'9223372036854775806'"]
        );
    }

    #[test]
    fn tildes() {
        assert_eq!(exp("~{,/tmp}"), ["<~>", "<~>/tmp"]);
        assert_eq!(exp("{~,~root}"), ["<~>", "<~root>"]);
        assert_eq!(exp("a{~,b}"), ["a~", "ab"]);
    }

    #[test]
    fn expanded_ends() {
        let param = |name: &str| {
            WordPart::Param(Box::new(crate::ast::ParamExp {
                name: crate::ast::ParamName::Var(name.as_bytes().to_vec()),
                index: None,
                op: crate::ast::ParamOp::Plain,
                colon: false,
                flags: None,
            }))
        };
        let mut calls = 0;
        let mut text = |w: &Word| -> Result<Vec<u8>, ()> {
            calls += 1;
            Ok(match &w.0[..] {
                [WordPart::Param(_)] => b"3".to_vec(),
                _ => b"x".to_vec(),
            })
        };
        let w = Word(
            vec![
                WordPart::Literal(b"{1..".to_vec()),
                param("n"),
                WordPart::Literal(b"}".to_vec()),
            ]
            .into(),
        );
        let words = expand(&w, &mut text).unwrap().unwrap();
        assert_eq!(words.len(), 3);
        // Not a sequence: the expansion is replaced by its text.
        let w = Word(
            vec![
                WordPart::Literal(b"{a..".to_vec()),
                param("n"),
                WordPart::Literal(b"}".to_vec()),
            ]
            .into(),
        );
        let words = expand(&w, &mut text).unwrap().unwrap();
        assert_eq!(
            words[0].0[..],
            [
                WordPart::Literal(b"{a..".to_vec()),
                WordPart::SingleQuoted(b"3".to_vec()),
                WordPart::Literal(b"}".to_vec())
            ]
        );
        assert_eq!(calls, 2);
    }
}
