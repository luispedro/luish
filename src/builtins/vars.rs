//! `export`, `readonly`, `unset`, `set`, `shift`, `local`.

use super::illegal_number;
use crate::lexer::is_valid_name;
use crate::options::{OPTIONS, Opt, Options};
use crate::shell::{ExecResult, Flow, Shell};

/// Single-quotes a value for output that can be read back by the shell.
pub fn single_quote(s: &[u8]) -> Vec<u8> {
    let mut out = vec![b'\''];
    for &c in s {
        if c == b'\'' {
            out.extend_from_slice(b"'\\''");
        } else {
            out.push(c);
        }
    }
    out.push(b'\'');
    out
}

fn bad_name(sh: &Shell, cmd: &[u8], name: &[u8]) -> Flow {
    sh.berr(cmd, format!("{}: bad variable name", String::from_utf8_lossy(name)));
    Flow::Error(2)
}

fn set_attr(sh: &mut Shell, argv: &[Vec<u8>], export: bool) -> ExecResult {
    let cmd = &argv[0];
    let mut args = &argv[1..];
    let mut print = false;
    while let Some(a) = args.first() {
        match a.as_slice() {
            b"-p" => print = true,
            b"--" => {
                args = &args[1..];
                break;
            }
            _ => break,
        }
        args = &args[1..];
    }
    if args.is_empty() {
        let prefix: &[u8] = if export { b"export " } else { b"readonly " };
        let mut out = Vec::new();
        for (name, var) in sh.vars.sorted() {
            if (export && var.exported) || (!export && var.readonly) {
                out.extend_from_slice(prefix);
                out.extend_from_slice(name);
                if let Some(v) = &var.value {
                    out.push(b'=');
                    out.extend(single_quote(v));
                }
                out.push(b'\n');
            }
        }
        let _ = print;
        return Ok(sh.out_or_err(cmd, &out));
    }
    for a in args {
        let (name, value) = match a.iter().position(|&c| c == b'=') {
            Some(i) => (&a[..i], Some(a[i + 1..].to_vec())),
            None => (&a[..], None),
        };
        if !is_valid_name(name) {
            return Err(bad_name(sh, cmd, name));
        }
        if let Some(v) = value {
            sh.set_var(name, v)?;
        }
        let var = sh.vars.entry(name);
        if export {
            var.exported = true;
        } else {
            var.readonly = true;
        }
    }
    Ok(0)
}

pub fn export(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    set_attr(sh, argv, true)
}

pub fn readonly(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    set_attr(sh, argv, false)
}

pub fn unset(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut functions = false;
    let mut args = &argv[1..];
    while let Some(a) = args.first() {
        match a.as_slice() {
            b"-f" => functions = true,
            b"-v" => functions = false,
            b"--" => {
                args = &args[1..];
                break;
            }
            _ => break,
        }
        args = &args[1..];
    }
    for name in args {
        if functions {
            sh.functions.remove(name);
            continue;
        }
        if sh.vars.unset(name).is_err() {
            sh.berr(&argv[0], format!("{}: is read only", String::from_utf8_lossy(name)));
            return Err(Flow::Error(2));
        }
        sh.var_changed(name);
    }
    Ok(0)
}

pub fn shift(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let n = match argv.get(1) {
        None => 1,
        Some(a) => match super::parse_uint(a) {
            Some(n) => n as usize,
            None => return Err(illegal_number(sh, &argv[0], a)),
        },
    };
    if n > sh.positional.len() {
        sh.berr(&argv[0], "can't shift that many");
        return Err(Flow::Error(2));
    }
    sh.positional.drain(..n);
    sh.reset_getopts();
    Ok(0)
}

fn print_vars(sh: &Shell) -> i32 {
    let mut out = Vec::new();
    for (name, var) in sh.vars.sorted() {
        if let Some(v) = &var.value {
            out.extend_from_slice(name);
            out.push(b'=');
            out.extend(single_quote(v));
            out.push(b'\n');
        }
    }
    sh.out_or_err(b"set", &out)
}

fn print_options(sh: &Shell, reinput: bool) -> i32 {
    let mut out = String::new();
    if !reinput {
        out.push_str("Current option settings\n");
    }
    for (o, _, name) in OPTIONS {
        let on = sh.options.get(*o);
        if reinput {
            out.push_str(&format!("set {}o {name}\n", if on { '-' } else { '+' }));
        } else {
            out.push_str(&format!("{name:<16}{}\n", if on { "on" } else { "off" }));
        }
    }
    sh.out_or_err(b"set", out.as_bytes())
}

/// Parses `set`-style option arguments (also used on the command line).
/// Returns the remaining (operand) arguments and whether `--` or `-` was
/// seen (meaning the positional parameters are replaced even if empty).
pub fn parse_set_options<'a>(sh: &mut Shell, args: &'a [Vec<u8>], cmd: &[u8]) -> Result<(&'a [Vec<u8>], bool), Flow> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.is_empty() || (a[0] != b'-' && a[0] != b'+') {
            break;
        }
        if a == b"--" {
            return Ok((&args[i + 1..], true));
        }
        if a == b"-" {
            sh.options.set(Opt::Xtrace, false);
            sh.options.set(Opt::Verbose, false);
            return Ok((&args[i + 1..], true));
        }
        if a == b"+" {
            i += 1;
            continue;
        }
        let on = a[0] == b'-';
        for &c in &a[1..] {
            if c == b'o' {
                i += 1;
                match args.get(i) {
                    None => {
                        print_options(sh, !on);
                    }
                    Some(name) => match Options::by_name(name) {
                        Some(o) => sh.options.set(o, on),
                        None => {
                            sh.berr(cmd, format!("Illegal option -o {}", String::from_utf8_lossy(name)));
                            return Err(Flow::Error(2));
                        }
                    },
                }
                continue;
            }
            match Options::by_letter(c) {
                Some(o) if !matches!(o, Opt::Interactive | Opt::Stdin) || cmd != b"set" => sh.options.set(o, on),
                _ => {
                    sh.berr(cmd, format!("Illegal option {}{}", a[0] as char, c as char));
                    return Err(Flow::Error(2));
                }
            }
        }
        i += 1;
    }
    Ok((&args[i.min(args.len())..], false))
}

pub fn set(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if argv.len() == 1 {
        return Ok(print_vars(sh));
    }
    let (rest, force) = parse_set_options(sh, &argv[1..], &argv[0])?;
    sh.set_jobctl(sh.opt(Opt::Monitor));
    if force || !rest.is_empty() {
        sh.positional = rest.to_vec();
        sh.reset_getopts();
    }
    Ok(0)
}

pub fn local(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if sh.locals.is_empty() {
        sh.berr(&argv[0], "not in a function");
        return Err(Flow::Error(2));
    }
    for a in &argv[1..] {
        let (name, value) = match a.iter().position(|&c| c == b'=') {
            Some(i) => (&a[..i], Some(a[i + 1..].to_vec())),
            None => (&a[..], None),
        };
        if name == b"-" {
            continue;
        }
        if !is_valid_name(name) {
            sh.berr(
                &argv[0],
                format!("{}: bad variable name", String::from_utf8_lossy(name)),
            );
            return Err(Flow::Error(2));
        }
        let frame = sh.locals.last().unwrap();
        if !frame.iter().any(|(n, _)| n == name) {
            let old = sh.vars.take(name);
            sh.locals.last_mut().unwrap().push((name.to_vec(), old));
        }
        if let Some(v) = value {
            sh.set_var(name, v)?;
        }
    }
    Ok(0)
}
