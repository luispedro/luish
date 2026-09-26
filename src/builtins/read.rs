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

pub fn getopts(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() < 3 {
        sh.berr(&argv[0], "usage: getopts optstring var [arg ...]");
        return Ok(2);
    }
    let optstring = argv[1].clone();
    let name = argv[2].clone();
    let args: Vec<Vec<u8>> = if argv.len() > 3 {
        argv[3..].to_vec()
    } else {
        sh.positional.clone()
    };
    let optind_var = sh.get_var(b"OPTIND").unwrap_or_default();
    if optind_var != sh.getopts_optind {
        sh.getopts_offset = 0;
    }
    let mut optind: usize = std::str::from_utf8(&optind_var)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1);
    let silent = optstring.first() == Some(&b':');
    let mut offset = sh.getopts_offset;

    let finish = |sh: &mut Shell, optind: usize, offset: usize| -> Result<(), crate::shell::Flow> {
        let s = optind.to_string().into_bytes();
        sh.vars.set(b"OPTIND", s.clone()).ok();
        sh.getopts_optind = s;
        sh.getopts_offset = offset;
        Ok(())
    };

    let end = |sh: &mut Shell, optind: usize| -> ExecResult {
        sh.set_var(&name, b"?".to_vec())?;
        let _ = sh.vars.unset(b"OPTARG");
        finish(sh, optind, 0)?;
        Ok(1)
    };

    if offset == 0 {
        let Some(arg) = args.get(optind - 1) else {
            return end(sh, optind);
        };
        if arg == b"--" {
            return end(sh, optind + 1);
        }
        if arg.len() < 2 || arg[0] != b'-' {
            return end(sh, optind);
        }
        offset = 1;
    }
    let arg = args[optind - 1].clone();
    let c = arg[offset];
    offset += 1;
    if offset >= arg.len() {
        optind += 1;
        offset = 0;
    }
    let spec = optstring.iter().position(|&o| o == c && c != b':');
    let mut optarg: Option<Vec<u8>> = None;
    let result: Vec<u8>;
    match spec {
        None => {
            if silent {
                optarg = Some(vec![c]);
            } else {
                sh.berr(&argv[0], format!("Illegal option -{}", c as char));
            }
            result = b"?".to_vec();
        }
        Some(p) if optstring.get(p + 1) == Some(&b':') => {
            if offset != 0 {
                optarg = Some(arg[offset..].to_vec());
                optind += 1;
                offset = 0;
                result = vec![c];
            } else if let Some(a) = args.get(optind - 1) {
                optarg = Some(a.clone());
                optind += 1;
                result = vec![c];
            } else if silent {
                optarg = Some(vec![c]);
                result = b":".to_vec();
            } else {
                sh.berr(&argv[0], format!("No arg for -{} option", c as char));
                result = b"?".to_vec();
            }
        }
        Some(_) => result = vec![c],
    }
    match optarg {
        Some(v) => sh.set_var(b"OPTARG", v)?,
        None => {
            let _ = sh.vars.unset(b"OPTARG");
        }
    }
    sh.set_var(&name, result)?;
    finish(sh, optind, offset)?;
    Ok(0)
}
