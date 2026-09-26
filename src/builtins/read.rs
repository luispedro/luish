//! `read` and `getopts`.

use crate::lexer::is_valid_name;
use crate::shell::{ExecResult, Shell};
use crate::sys;

/// Reads one line from fd 0 a byte at a time, so that nothing after the
/// newline is consumed. Returns the bytes (each with an "escaped" flag) and
/// whether end of file was reached before a newline.
fn read_line(raw: bool) -> (Vec<(u8, bool)>, bool) {
    let mut out = Vec::new();
    let mut buf = [0u8; 1];
    let mut escape = false;
    loop {
        match sys::read(0, &mut buf, false) {
            Ok(1) => {}
            _ => return (out, true),
        }
        let c = buf[0];
        if c == 0 {
            // dash drops NUL bytes.
            continue;
        }
        if escape {
            escape = false;
            if c != b'\n' {
                out.push((c, true));
            }
            continue;
        }
        if c == b'\\' && !raw {
            escape = true;
            continue;
        }
        if c == b'\n' {
            return (out, false);
        }
        out.push((c, false));
    }
}

pub fn read(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut raw = false;
    let mut prompt = None;
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == b"--" {
            i += 1;
            break;
        }
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        let mut j = 1;
        while j < a.len() {
            match a[j] {
                b'r' => raw = true,
                b'p' => {
                    if j + 1 < a.len() {
                        prompt = Some(a[j + 1..].to_vec());
                    } else {
                        i += 1;
                        prompt = argv.get(i).cloned();
                    }
                    j = a.len();
                    continue;
                }
                c => {
                    sh.berr(&argv[0], format!("Illegal option -{}", c as char));
                    return Ok(2);
                }
            }
            j += 1;
        }
        i += 1;
    }
    let names = &argv[i..];
    if names.is_empty() {
        sh.berr(&argv[0], "arg count");
        return Ok(2);
    }
    for n in names {
        if !is_valid_name(n) {
            sh.berr(&argv[0], format!("{}: bad variable name", String::from_utf8_lossy(n)));
            return Ok(2);
        }
    }
    if let Some(p) = prompt
        && sys::isatty(0)
    {
        sys::write_all(2, &p);
    }
    let (line, eof) = read_line(raw);
    let ifs = sh.get_var(b"IFS").unwrap_or_else(|| b" \t\n".to_vec());
    let is_ifs = |c: &(u8, bool)| !c.1 && ifs.contains(&c.0);
    let is_ws = |c: &(u8, bool)| is_ifs(c) && matches!(c.0, b' ' | b'\t' | b'\n');
    let mut pos = 0;
    while pos < line.len() && is_ws(&line[pos]) {
        pos += 1;
    }
    for (k, name) in names.iter().enumerate() {
        let value: Vec<u8> = if k + 1 == names.len() {
            // The last variable gets the rest of the line, minus trailing
            // IFS whitespace.
            let mut end = line.len();
            while end > pos && is_ws(&line[end - 1]) {
                end -= 1;
            }
            let v = line[pos.min(end)..end].iter().map(|c| c.0).collect();
            pos = line.len();
            v
        } else {
            let start = pos;
            while pos < line.len() && !is_ifs(&line[pos]) {
                pos += 1;
            }
            let v = line[start..pos].iter().map(|c| c.0).collect();
            // Skip the delimiter: whitespace, at most one non-whitespace
            // IFS character, then whitespace.
            while pos < line.len() && is_ws(&line[pos]) {
                pos += 1;
            }
            if pos < line.len() && is_ifs(&line[pos]) && !is_ws(&line[pos]) {
                pos += 1;
                while pos < line.len() && is_ws(&line[pos]) {
                    pos += 1;
                }
            }
            v
        };
        sh.set_var(name, value)?;
    }
    Ok(if eof { 1 } else { 0 })
}

/// A port of dash's `getopts`. The position is `sh.optind` (the next
/// argument, as in `$OPTIND`) and `sh.optoff` (the offset in the previous
/// argument when it has more option letters); assigning `OPTIND`, `set --`,
/// `shift` and function calls reset it.
pub fn getopts(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() < 3 {
        sh.berr(&argv[0], "Usage: getopts optstring var [arg...]");
        return Ok(2);
    }
    let (optstr, optvar) = (&argv[1], &argv[2]);
    if !crate::lexer::is_valid_name(optvar) {
        sh.berr(
            &argv[0],
            format!("{}: bad variable name", String::from_utf8_lossy(optvar)),
        );
        return Ok(2);
    }
    let base: Vec<Vec<u8>> = if argv.len() == 3 {
        sh.positional.clone()
    } else {
        argv[3..].to_vec()
    };
    if sh.optind > base.len() + 1 {
        sh.optind = 1;
        sh.optoff = None;
    }
    let ind = sh.optind;
    let off = sh.optoff;
    // `next`: the index of the next argument; `p`: the position of the
    // next option letter in `base[next - 1]`.
    let mut next = ind - 1;
    let mut p = match off {
        Some(o) if ind > 1 && base[next - 1].len() >= o => Some(o),
        _ => None,
    };
    let mut c = b'?';
    let mut done = false;
    'out: {
        if p.is_none_or(|o| o == base[next - 1].len()) {
            // The current argument is done: advance.
            match base.get(next) {
                Some(w) if w.len() > 1 && w[0] == b'-' => {
                    next += 1;
                    p = Some(1);
                    if w == b"--" {
                        p = None;
                        done = true;
                        break 'out;
                    }
                }
                _ => {
                    p = None;
                    done = true;
                    break 'out;
                }
            }
        }
        let word = &base[next - 1];
        let mut o = p.unwrap();
        c = word[o];
        o += 1;
        p = Some(o);
        let mut q = 0;
        while optstr.get(q) != Some(&c) {
            if q >= optstr.len() {
                if optstr.first() == Some(&b':') {
                    sh.set_var(b"OPTARG", vec![c])?;
                } else {
                    sys::write_all(2, format!("Illegal option -{}\n", c as char).as_bytes());
                    let _ = sh.vars.unset(b"OPTARG");
                }
                c = b'?';
                break 'out;
            }
            q += 1;
            if optstr.get(q) == Some(&b':') {
                q += 1;
            }
        }
        if optstr.get(q + 1) == Some(&b':') {
            let arg = if o < word.len() {
                word[o..].to_vec()
            } else if let Some(w) = base.get(next) {
                next += 1;
                w.clone()
            } else {
                if optstr.first() == Some(&b':') {
                    sh.set_var(b"OPTARG", vec![c])?;
                    c = b':';
                } else {
                    sys::write_all(2, format!("No arg for -{} option\n", c as char).as_bytes());
                    let _ = sh.vars.unset(b"OPTARG");
                    c = b'?';
                }
                break 'out;
            };
            sh.set_var(b"OPTARG", arg)?;
            p = None;
        } else {
            sh.set_var(b"OPTARG", Vec::new())?;
        }
    }
    let ind = next + 1;
    sh.set_var(b"OPTIND", ind.to_string().into_bytes())?;
    sh.set_var(optvar, vec![c])?;
    sh.optoff = p;
    sh.optind = ind;
    Ok(done as i32)
}
