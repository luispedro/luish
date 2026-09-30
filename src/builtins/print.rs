//! `print`, as in zsh: `echo` with options, among them `-P` for prompt
//! expansion (`prompt.rs`).
//!
//! `print` is a built-in only in interactive shells (see `INTERACTIVE` in
//! `mod.rs`), so that scripts find the same commands as in dash;
//! `__luish_internal print` is the same command everywhere.

use crate::shell::{ExecResult, Shell};
use crate::sys;

/// The options that take an argument, attached or as the next word.
const WITH_ARG: &[u8] = b"CfuvxX";
/// The options that take none.
const FLAGS: &[u8] = b"abcDilmnNoOpPrRsz";

/// The options given: `flags` are letters, `args` the values of those that
/// take one (the last of each wins).
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
        self.args.iter().rev().find(|a| a.0 == c).map(|a| a.1.as_slice())
    }
}

pub fn print(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, b"print", &argv[1..])
}

/// `print` with the arguments `args`, reporting errors as `name`.
pub fn run(sh: &mut Shell, name: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let (o, args) = match options(sh, name, args) {
        Ok(r) => r,
        Err(status) => return Ok(status),
    };
    let err = |msg: String| {
        sh.berr(name, msg);
        Ok(1)
    };
    let number = |c: u8| -> Result<Option<usize>, String> {
        let Some(a) = o.arg(c) else { return Ok(None) };
        let s = String::from_utf8_lossy(a);
        match (c, s.parse::<usize>()) {
            (b'C', Ok(0)) => Err(format!("invalid number of columns: {s}")),
            (b'x' | b'X', Ok(0) | Err(_)) => Err(format!("positive integer expected after -{}: {s}", c as char)),
            (_, Err(_)) => Err(format!("number expected after -{}: {s}", c as char)),
            (_, Ok(n)) => Ok(Some(n)),
        }
    };
    let (columns, fd, all_tabs, tabs) = match (number(b'C'), number(b'u'), number(b'X'), number(b'x')) {
        (Ok(c), Ok(u), Ok(xx), Ok(x)) => (c, u, xx, x),
        (Err(e), ..) | (_, Err(e), ..) | (.., Err(e), _) | (.., Err(e)) => return err(e),
    };
    let tabs = all_tabs.map(|n| (n, true)).or(tabs.map(|n| (n, false)));
    let fd = fd.map_or(1, |n| n as i32);
    if !sys::fd_is_open(fd) {
        return err(format!("bad file number: {fd}"));
    }
    if o.has(b'p') {
        return err("-p: no coprocess".into());
    }
    let var = o.arg(b'v');
    if let Some(v) = var
        && !crate::lexer::is_valid_name(v)
    {
        return err(format!("not an identifier: {}", String::from_utf8_lossy(v)));
    }

    let mut args = args.to_vec();
    if o.has(b'm') {
        if args.is_empty() {
            return err("no pattern specified".into());
        }
        let pat = super::misc::name_pattern(&args.remove(0));
        args.retain(|a| pat.matches(a));
    }
    // Escapes (unless `-r`, or `-R` without `-e`, or with `-f`), then
    // prompt expansion, then `~` for `HOME`, each argument in turn. `\c`
    // drops the rest of the output, and the newline.
    let escapes = !o.has(b'r') && (!o.has(b'R') || o.has(b'e')) && o.arg(b'f').is_none();
    let home = if o.has(b'D') { sh.get_var(b"HOME") } else { None };
    let mut stop = false;
    for i in 0..args.len() {
        if escapes {
            let mut out = Vec::with_capacity(args[i].len());
            stop = print_escapes(&args[i], o.has(b'b'), &mut out);
            args[i] = out;
        }
        if o.has(b'P') {
            args[i] = crate::prompt::expand(sh, &args[i]).text;
        }
        if o.has(b'D') {
            args[i] = super::dirstack::abbreviate(home.as_deref(), &args[i]);
        }
        if stop {
            args.truncate(i + 1);
            break;
        }
    }
    if o.has(b'o') || o.has(b'O') {
        let key = |a: &Vec<u8>| if o.has(b'i') { a.to_ascii_lowercase() } else { a.clone() };
        // Stable, so that equal ones keep their order either way, as in zsh.
        match o.has(b'O') {
            false => args.sort_by_cached_key(key),
            true => args.sort_by_cached_key(|a| std::cmp::Reverse(key(a))),
        }
    }
    if o.has(b'z') || o.has(b's') {
        let text = String::from_utf8_lossy(&args.join(&b' ')).into_owned();
        if o.has(b'z') {
            crate::interactive::push_buffer(text);
        } else {
            crate::interactive::add_history_entry(sh, &text);
        }
        return Ok(0);
    }

    let mut status = 0;
    let out = if let Some(fmt) = o.arg(b'f') {
        let (out, s) = super::printf::format(sh, fmt, &args);
        status = s;
        out
    } else if columns.is_some() || o.has(b'c') {
        let width = columns.is_none().then(|| terminal_width(sh, fd));
        in_columns(&args, columns, width, o.has(b'a'), if o.has(b'N') { 0 } else { b'\n' })
    } else {
        let sep = if o.has(b'l') {
            b'\n'
        } else if o.has(b'N') {
            0
        } else {
            b' '
        };
        let mut out = Vec::new();
        let mut col = 0;
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                out.push(sep);
                col = if sep == b'\n' { 0 } else { col + 1 };
            }
            match tabs {
                Some((stop, all)) => expand_tabs(a, stop, all, &mut col, &mut out),
                None => out.extend_from_slice(a),
            }
        }
        // As in zsh, `-v` stores the text without the newline, unless `-l`
        // is given.
        if !stop && !o.has(b'n') && (var.is_none() || o.has(b'l')) {
            out.push(if o.has(b'N') { 0 } else { b'\n' });
        }
        out
    };
    if let Some(v) = var {
        if let Err(msg) = sh.try_set_var(v, out) {
            sh.error(msg);
            return Ok(1);
        }
        return Ok(status);
    }
    if fd == 1 {
        sh.out(&out);
    } else if !sys::write_all(fd, &out) {
        return err(format!("write error: {}", sys::strerror(sys::errno())));
    }
    Ok(status)
}

/// Parses the options, as zsh does for `print`: letters can be grouped,
/// an option's argument is the rest of its word or the next word, `-` or
/// `--` ends the options, and so does a word such as `-1`. After `-R`
/// only `-e` and `-n` are options (in words of their own), as in BSD
/// `echo`. Returns the options and the operands, or the status after an
/// error.
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
        if o.has(b'R') {
            if !a[1..].iter().all(|c| b"en".contains(c)) {
                break;
            }
            o.flags.extend_from_slice(&a[1..]);
            i += 1;
            continue;
        }
        if a == b"--" {
            i += 1;
            break;
        }
        if a[1].is_ascii_digit() {
            break;
        }
        i += 1;
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

/// zsh's escapes for `print` (its `getkeystring`), which differ from
/// `echo`'s: `\NNN` is octal without a leading 0, `\x`, `\u` and `\U` take
/// hex digits, `\M-` and `\C-` make meta and control characters, and an
/// unknown escape is the character itself. With `bindkey`, `^X` is a
/// control character too, and `\c` is `c`. Returns true if `\c` was seen
/// (stop all output).
fn print_escapes(s: &[u8], bindkey: bool, out: &mut Vec<u8>) -> bool {
    let mut i = 0;
    while i < s.len() {
        match key(s, &mut i, bindkey) {
            Some(Key::Byte(b)) => out.push(b),
            Some(Key::Char(c)) => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            Some(Key::Stop) => return true,
            None => {}
        }
    }
    false
}

enum Key {
    Byte(u8),
    Char(char),
    Stop,
}

/// Reads one character of `s` from `i`, or an escape that stands for one.
fn key(s: &[u8], i: &mut usize, bindkey: bool) -> Option<Key> {
    let c = *s.get(*i)?;
    *i += 1;
    if bindkey && c == b'^' && *i < s.len() {
        let c = s[*i];
        *i += 1;
        return Some(Key::Byte(control(c)));
    }
    if c != b'\\' || *i == s.len() {
        return Some(Key::Byte(c));
    }
    let e = s[*i];
    *i += 1;
    // Up to `max` digits in base `radix`: 0 if there are none.
    let digits = |i: &mut usize, radix: u32, max: usize| {
        let mut v: u32 = 0;
        for _ in 0..max {
            match s.get(*i).and_then(|&c| (c as char).to_digit(radix)) {
                Some(d) => v = v * radix + d,
                None => break,
            }
            *i += 1;
        }
        v
    };
    let b = match e {
        b'a' => 7,
        b'b' => 8,
        b'e' | b'E' => 0x1b,
        b'f' => 12,
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'v' => 11,
        b'c' if !bindkey => return Some(Key::Stop),
        b'x' => digits(i, 16, 2) as u8,
        b'u' | b'U' => {
            let v = digits(i, 16, if e == b'u' { 4 } else { 8 });
            return char::from_u32(v).map(Key::Char);
        }
        b'0'..=b'7' => {
            *i -= 1;
            digits(i, 8, 3) as u8
        }
        b'M' | b'C' => {
            if s.get(*i) == Some(&b'-') {
                *i += 1;
            }
            return match key(s, i, bindkey)? {
                Key::Byte(b) if e == b'M' => Some(Key::Byte(b | 0x80)),
                Key::Byte(b) => Some(Key::Byte(control(b))),
                k => Some(k),
            };
        }
        e => e,
    };
    Some(Key::Byte(b))
}

/// The control character for `c`: `^?` is DEL.
fn control(c: u8) -> u8 {
    if c == b'?' { 0x7f } else { c & 0x9f }
}

/// Appends `s` with tabs expanded to stops every `stop` columns: all of
/// them, or only those at the start of a line. `col` is the column,
/// carried from one argument to the next.
fn expand_tabs(s: &[u8], stop: usize, all: bool, col: &mut usize, out: &mut Vec<u8>) {
    let mut leading = true;
    for &c in s {
        match c {
            b'\t' if all || leading => {
                let n = stop - *col % stop;
                out.extend(std::iter::repeat_n(b' ', n));
                *col += n;
            }
            b'\n' => {
                out.push(c);
                *col = 0;
                leading = true;
            }
            _ => {
                out.push(c);
                if c & 0xc0 != 0x80 {
                    *col += 1;
                }
                leading &= c == b'\t';
            }
        }
    }
}

/// The number of columns for `-c`: `COLUMNS`, or else the width of the
/// terminal on `fd`, or else 80.
fn terminal_width(sh: &Shell, fd: i32) -> usize {
    sh.get_var(b"COLUMNS")
        .and_then(|c| std::str::from_utf8(&c).ok()?.trim().parse().ok())
        .filter(|&n| n > 0)
        .or_else(|| sys::window_size(fd).map(|s| s.0).filter(|&n| n > 0))
        .unwrap_or(80)
}

/// The arguments in columns, as zsh's `print -c` (the number of columns
/// that fit in `width`) and `-C n`: sorted down the columns, or across
/// them with `across`. Columns are two spaces wider than the longest
/// argument in them, leaving out the last column, which may be ragged.
/// Each row ends with `end`.
fn in_columns(args: &[Vec<u8>], cols: Option<usize>, width: Option<usize>, across: bool, end: u8) -> Vec<u8> {
    let chars = |a: &[u8]| a.iter().filter(|&&c| c & 0xc0 != 0x80).count();
    let n = args.len();
    let (nc, cell) = match cols {
        Some(nc) => {
            let rows = n.div_ceil(nc);
            let in_last = |i: usize| if across { i % nc == nc - 1 } else { i >= rows * (nc - 1) };
            let longest = (0..n).filter(|&i| !in_last(i)).map(|i| chars(&args[i])).max();
            (nc, longest.unwrap_or(0) + 2)
        }
        None => {
            let cell = args.iter().map(|a| chars(a)).max().unwrap_or(0) + 2;
            (((width.unwrap_or(80) + 1) / cell).max(1), cell)
        }
    };
    let rows = n.div_ceil(nc);
    let mut out = Vec::new();
    let cell_of = |out: &mut Vec<u8>, i: usize, pad: bool| {
        out.extend_from_slice(&args[i]);
        if pad {
            out.extend(std::iter::repeat_n(b' ', cell.saturating_sub(chars(&args[i]))));
        }
    };
    for r in 0..rows {
        if across {
            // As in zsh, every cell but the last of a row is padded, even
            // the last argument.
            for i in (r * nc..n).take(nc) {
                cell_of(&mut out, i, i % nc != nc - 1);
            }
        } else {
            for i in (r..n).step_by(rows) {
                cell_of(&mut out, i, i + rows < n);
            }
        }
        out.push(end);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esc(s: &str, bindkey: bool) -> (Vec<u8>, bool) {
        let mut out = Vec::new();
        let stop = print_escapes(s.as_bytes(), bindkey, &mut out);
        (out, stop)
    }

    #[test]
    fn escapes() {
        assert_eq!(esc(r"a\tb\\", false).0, b"a\tb\\");
        assert_eq!(esc(r"\101\0101\x41\x4g\xg", false).0, b"A\x081A\x04g\0g");
        assert_eq!(esc(r"\777\400\1a", false).0, b"\xff\0\x01a");
        assert_eq!(esc(r"é\U0001F600\uzz", false).0, "é😀\0zz".as_bytes());
        assert_eq!(esc(r"\M-\C-a\C-?\Mx\Cx\M-", false).0, b"\x81\x7f\xf8\x18");
        assert_eq!(esc(r#"\q\"\E\"#, false).0, b"q\"\x1b\\");
        assert_eq!(esc(r"a\cb", false), (b"a".to_vec(), true));
        assert_eq!(esc(r"^a^?\cb^", true), (b"\x01\x7fcb^".to_vec(), false));
        assert_eq!(esc(r"^a", false).0, b"^a");
    }

    #[test]
    fn tabs() {
        let t = |s: &str, all| {
            let mut out = Vec::new();
            expand_tabs(s.as_bytes(), 4, all, &mut 2, &mut out);
            String::from_utf8(out).unwrap()
        };
        assert_eq!(t("\tb\tc\n\t\td", false), "  b\tc\n        d");
        assert_eq!(t("\tb\tc\n\td", true), "  b   c\n    d");
    }

    #[test]
    fn columns() {
        let args: Vec<Vec<u8>> = ["1", "2", "3", "4", "5"]
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
        let c = |cols, width, across| String::from_utf8(in_columns(&args, cols, width, across, b'\n')).unwrap();
        assert_eq!(c(Some(2), None, false), "1  4\n2  5\n3\n");
        assert_eq!(c(Some(2), None, true), "1  2\n3  4\n5  \n");
        assert_eq!(c(None, Some(9), false), "1  3  5\n2  4\n");
        assert_eq!(c(None, Some(1), false), "1\n2\n3\n4\n5\n");
        assert_eq!(in_columns(&[], Some(2), None, false, b'\n'), b"");
    }
}
