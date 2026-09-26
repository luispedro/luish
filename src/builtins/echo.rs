//! `echo`, compatible with dash: only `-n`, and XSI escapes always on.

use crate::shell::{ExecResult, Shell};

/// Expands backslash escapes as `echo` and `printf %b` do. Returns true if
/// `\c` was seen (stop all output).
pub fn echo_escapes(s: &[u8], out: &mut Vec<u8>) -> bool {
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c != b'\\' || i + 1 == s.len() {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let e = s[i];
        i += 1;
        match e {
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'c' => return true,
            b'f' => out.push(12),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(11),
            b'\\' => out.push(b'\\'),
            // Debian's dash has `\e`.
            b'e' => out.push(0x1b),
            // As in dash: `\0` then up to three octal digits, or up to three
            // octal digits starting with 1-7.
            b'0'..=b'7' => {
                let mut v: u32 = 0;
                let mut n = 0;
                if e != b'0' || !s.get(i).is_some_and(|c| (b'0'..=b'7').contains(c)) {
                    i -= 1;
                }
                while n < 3 && i < s.len() && (b'0'..=b'7').contains(&s[i]) {
                    v = v * 8 + (s[i] - b'0') as u32;
                    i += 1;
                    n += 1;
                }
                out.push(v as u8);
            }
            _ => {
                out.push(b'\\');
                out.push(e);
            }
        }
    }
    false
}

pub fn echo(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut args = &argv[1..];
    let mut newline = true;
    if args.first().is_some_and(|a| a == b"-n") {
        newline = false;
        args = &args[1..];
    }
    let mut out = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        if echo_escapes(a, &mut out) {
            newline = false;
            break;
        }
    }
    if newline {
        out.push(b'\n');
    }
    Ok(sh.out_or_err(&argv[0], &out))
}
