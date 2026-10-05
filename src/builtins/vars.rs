//! `export`, `readonly`, `unset`, `set`, `shift`, `local`, `typeset`.

use super::illegal_number;
use crate::exec::AssignValue;
use crate::lexer::is_valid_name;
use crate::options::{EXTENDED, Kind, OPTIONS, Opt, Options, Setting, VALUES, find_group, group_of, parse_bool};
use crate::shell::{ExecResult, Flow, Shell};
use crate::vars::{Item, Special, Subscript, Transform, Value};

/// Single-quotes a value for output that can be read back by the shell.
/// dash's `single_quote`: the text in single quotes, with each run of
/// single quotes in double quotes (`a'b` is `'a'"'"'b'`).
pub fn single_quote(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    let mut rest = s;
    loop {
        let len = rest.iter().position(|&c| c == b'\'').unwrap_or(rest.len());
        out.push(b'\'');
        out.extend_from_slice(&rest[..len]);
        out.push(b'\'');
        rest = &rest[len..];
        if rest.is_empty() {
            return out;
        }
        let quotes = rest.iter().take_while(|&&c| c == b'\'').count();
        out.push(b'"');
        out.extend_from_slice(&rest[..quotes]);
        out.push(b'"');
        rest = &rest[quotes..];
        if rest.is_empty() {
            return out;
        }
    }
}

/// A value quoted so that the shell reads it back: an array as `('a' 'b')`,
/// and an associative array as `(['k']='v')`.
pub fn quote_value(v: &Value) -> Vec<u8> {
    let mut out = vec![b'('];
    match v {
        Value::Str(s) => return single_quote(s),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.extend(single_quote(item));
            }
        }
        Value::Assoc(h) => {
            for (i, (k, v)) in h.keys().iter().zip(h.values()).enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.push(b'[');
                out.extend(single_quote(k));
                out.extend_from_slice(b"]=");
                out.extend(single_quote(v));
            }
        }
    }
    out.push(b')');
    out
}

/// Splits a `name=value` argument. A declaration command's argument
/// `name=(x [k]=y)` comes as `name=`, a NUL, and each element as `=x`, with
/// a key before it as `[k`, each followed by a NUL (`expand_command_words`):
/// an argument can't otherwise have a NUL.
pub fn split_arg(a: &[u8]) -> (&[u8], Option<AssignValue>) {
    let Some(i) = a.iter().position(|&c| c == b'=') else {
        return (a, None);
    };
    let value = match a[i + 1..].split_first() {
        Some((0, rest)) => {
            let mut items = Vec::new();
            let mut key = None;
            for field in rest.split(|&c| c == 0) {
                match field.split_first() {
                    Some((b'[', k)) => key = Some(k.to_vec()),
                    Some((_, v)) => items.push(Item {
                        key: key.take(),
                        value: v.to_vec(),
                    }),
                    // After the last NUL.
                    None => {}
                }
            }
            AssignValue::Items(items)
        }
        _ => AssignValue::Str(a[i + 1..].to_vec()),
    };
    (&a[..i], Some(value))
}

/// Assigns the value of a declaration command's argument.
fn assign_arg(sh: &mut Shell, name: &[u8], value: AssignValue) -> Result<(), Flow> {
    match value {
        AssignValue::Str(s) => sh.set_var(name, s),
        AssignValue::Items(items) => sh.assign_items(name, items, false),
    }
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
                    out.extend(quote_value(v));
                }
                out.push(b'\n');
            }
        }
        let _ = print;
        return Ok(sh.out_status(&out));
    }
    for a in args {
        let (name, value) = split_arg(a);
        if !is_valid_name(name) {
            return Err(bad_name(sh, cmd, name));
        }
        if let Some(v) = value {
            assign_arg(sh, name, v)?;
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
        // `unset 'a[i]'` empties an element, as in zsh (bash removes it,
        // leaving a hole); `unset 'a[@]'` unsets the array, as in bash.
        let name = match name.split_last() {
            Some((b']', rest)) if let Some(open) = rest.iter().position(|&c| c == b'[') => {
                let (base, index) = (&rest[..open], &rest[open + 1..]);
                if !is_valid_name(base) {
                    return Err(bad_name(sh, &argv[0], name));
                }
                if index != b"@" && index != b"*" {
                    unset_element(sh, &argv[0], base, index)?;
                    continue;
                }
                base
            }
            _ => &name[..],
        };
        if !is_valid_name(name) {
            return Err(bad_name(sh, &argv[0], name));
        }
        if name == b"OPTIND" {
            // dash resets getopts with the empty value, which isn't a number.
            sh.berr(&argv[0], "Illegal number: ");
            return Err(Flow::Error(2));
        }
        if sh.unset_var(name).is_err() {
            sh.berr(&argv[0], format!("{}: is read only", String::from_utf8_lossy(name)));
            return Err(Flow::Error(2));
        }
        sh.var_changed(name);
    }
    Ok(0)
}

/// `unset 'name[index]'`: empties the element, if there is one, or
/// removes the key of an associative array.
fn unset_element(sh: &mut Shell, cmd: &[u8], name: &[u8], index: &[u8]) -> Result<(), Flow> {
    if sh.vars.is_assoc(name) {
        if sh.vars.var(name).is_some_and(|v| v.readonly) {
            sh.berr(cmd, format!("{}: is read only", String::from_utf8_lossy(name)));
            return Err(Flow::Error(2));
        }
        if let Some(Value::Assoc(h)) = sh.vars.get_value_mut(name) {
            h.remove(index);
        }
        if sh.vartrace.is_some() {
            sh.trace_set(name);
        }
        return Ok(());
    }
    let i = crate::expand::arith::eval(sh, index).map_err(|msg| {
        sh.berr(cmd, msg);
        Flow::Error(2)
    })?;
    let len = match sh.vars.get_value(name) {
        Some(v) => v.elements().len(),
        None if sh.vars.special(name).is_some_and(Special::is_tied) => sh.special_elements(name).map_or(0, |v| v.len()),
        None => 0,
    } as i64;
    if (-len..len).contains(&i) {
        sh.set_element(name, &Subscript::Index(i), Vec::new(), false)?;
    }
    Ok(())
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
            out.extend(quote_value(v));
            out.push(b'\n');
        }
    }
    sh.out_status(&out)
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
    sh.out_status(out.as_bytes())
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
            // As in dash: turns off -x and -v, and ends the options; the
            // parameters are set only if some follow.
            sh.options.set(Opt::Xtrace, false);
            sh.options.set(Opt::Verbose, false);
            return Ok((&args[i + 1..], false));
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
                        Some(o) => sh.set_option(o, on),
                        None => {
                            sh.berr(cmd, format!("Illegal option -o {}", String::from_utf8_lossy(name)));
                            return Err(Flow::Error(2));
                        }
                    },
                }
                continue;
            }
            match Options::by_letter(c) {
                Some(o) if !matches!(o, Opt::Interactive | Opt::Stdin) || cmd != b"set" => sh.set_option(o, on),
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

/// `setopt` and `unsetopt` (not POSIX; as in zsh): set or unset options by
/// name, luish's own ones as well as dash's, and `setopt NAME=VALUE` sets
/// a setting (an option to `true` or `false`, or a setting with a value).
/// Without arguments, list the options that are on (`setopt`) or off
/// (`unsetopt`). `-p GROUP` puts the names in `GROUP`, or without names
/// lists the group's settings.
pub fn setopt(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let on = argv[0] == b"setopt";
    // `-p GROUP`: the names are in `GROUP` (the last one given counts).
    let mut group = None;
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        if a.len() < 2 || a[0] != b'-' {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        if a[1] != b'p' {
            sh.berr(&argv[0], format!("Illegal option -{}", a[1] as char));
            return Ok(2);
        }
        let name = if a.len() > 2 {
            &a[2..]
        } else if let Some(g) = argv.get(i) {
            i += 1;
            &g[..]
        } else {
            sh.berr(&argv[0], "No arg for -p option");
            return Ok(2);
        };
        match find_group(name) {
            Some(g) => group = Some(g),
            None => {
                let msg = format!("no such group: {}", String::from_utf8_lossy(name));
                sh.berr(&argv[0], msg);
                return Ok(1);
            }
        }
    }
    let args = &argv[i..];
    if args.is_empty() {
        if let Some(g) = group {
            return Ok(list_group(sh, g));
        }
        let mut names: Vec<&str> = Options::all_names()
            .filter(|&(o, _)| sh.options.get(o) == on)
            .map(|o| o.1)
            .collect();
        names.sort_unstable();
        let out: String = names.iter().map(|n| format!("{n}\n")).collect();
        return Ok(sh.out_status(out.as_bytes()));
    }
    let mut status = 0;
    for a in args {
        let full;
        let a = match group {
            Some(g) => {
                full = [g.as_bytes(), b".", a].concat();
                &full
            }
            None => a,
        };
        if let Err(msg) = set_setting(sh, on, a) {
            sh.berr(&argv[0], msg);
            status = 1;
        }
    }
    sh.set_jobctl(sh.opt(Opt::Monitor));
    Ok(status)
}

/// `setopt -p GROUP` without names: prints the group's settings, sorted,
/// as the commands that set them (the settings with a value only if their
/// variable is set).
fn list_group(sh: &Shell, group: &str) -> i32 {
    let in_group = |name: &str| group_of(name) == Some(group);
    let mut lines: Vec<(&str, Vec<u8>)> = Vec::new();
    for &(o, name) in EXTENDED.iter().filter(|o| in_group(o.1)) {
        let cmd = if sh.opt(o) { "setopt" } else { "unsetopt" };
        lines.push((name, format!("{cmd} {name}\n").into_bytes()));
    }
    for &(name, var, _) in VALUES.iter().filter(|v| in_group(v.0)) {
        if let Some(v) = sh.get_var(var) {
            let line = [format!("setopt {name}=").as_bytes(), &single_quote(&v), b"\n"].concat();
            lines.push((name, line));
        }
    }
    lines.sort_unstable_by_key(|l| l.0);
    let out: Vec<u8> = lines.into_iter().flat_map(|l| l.1).collect();
    sh.out_status(&out)
}

/// One argument of `setopt` (`on`) or `unsetopt`: `NAME` or `NAME=VALUE`.
fn set_setting(sh: &mut Shell, on: bool, arg: &[u8]) -> Result<(), String> {
    let (name, value) = match arg.iter().position(|&c| c == b'=') {
        Some(i) => (&arg[..i], Some(&arg[i + 1..])),
        None => (arg, None),
    };
    let text = |s: &[u8]| String::from_utf8_lossy(s).into_owned();
    let bad_value = |v: &[u8]| format!("{}: bad value: {}", text(name), text(v));
    if !on && value.is_some() {
        return Err(format!("{}: can't give a value", text(arg)));
    }
    match Options::find(name) {
        None => Err(format!("no such option: {}", text(name))),
        Some(Setting::Flag(Opt::Interactive | Opt::Stdin, _)) => Err(format!("can't change option: {}", text(name))),
        Some(Setting::Flag(o, sense)) => {
            let v = match value {
                Some(v) => parse_bool(v).ok_or_else(|| bad_value(v))?,
                None => on,
            };
            sh.set_option(o, v == sense);
            Ok(())
        }
        Some(Setting::Value(var, kind)) => match value {
            None if on => Err(format!("{}: needs a value", text(name))),
            None => sh.unset_var(var).map_err(|_| format!("{}: is read only", text(var))),
            Some(v) => {
                if kind == Kind::Number && (v.is_empty() || !v.iter().all(u8::is_ascii_digit)) {
                    return Err(bad_value(v));
                }
                sh.try_set_var(var, v.to_vec())
            }
        },
    }
}

pub fn local(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    if sh.locals.is_empty() {
        sh.berr(&argv[0], "not in a function");
        return Err(Flow::Error(2));
    }
    declare(sh, argv, true)
}

/// `typeset` and `declare` (not POSIX; as in zsh and bash).
pub fn typeset(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    declare(sh, argv, false)
}

/// The attributes that `typeset` and `local` set (`-a`, `-A`, `-i`, `-l`,
/// `-r`, `-u`, `-U`, `-x`) and remove (`+i`, `+l`...).
#[derive(Default)]
struct Attrs {
    array: bool,
    assoc: bool,
    integer: Option<bool>,
    lower: Option<bool>,
    upper: Option<bool>,
    unique: Option<bool>,
    readonly: Option<bool>,
    export: Option<bool>,
}

impl Attrs {
    /// Whether no attribute is given (plain `local x=y` or `typeset x`).
    fn is_empty(&self) -> bool {
        let a = self;
        let flags = [a.integer, a.lower, a.upper, a.unique, a.readonly, a.export];
        !a.array && !a.assoc && flags.iter().all(Option::is_none)
    }

    /// The attributes that change values, `old` changed by these. `-l` and
    /// `-u` replace each other, and together give neither (zsh and bash).
    fn transform(&self, old: Transform) -> Transform {
        let mut t = old;
        t.integer = self.integer.unwrap_or(t.integer);
        t.unique = self.unique.unwrap_or(t.unique);
        match (self.lower, self.upper) {
            (Some(true), Some(true)) => (t.lower, t.upper) = (false, false),
            (Some(true), _) => (t.lower, t.upper) = (true, false),
            (_, Some(true)) => (t.lower, t.upper) = (false, true),
            (l, u) => (t.lower, t.upper) = (l.unwrap_or(t.lower), u.unwrap_or(t.upper)),
        }
        t
    }
}

/// `local` and `typeset`. In a function (unless `-g`), each variable is made
/// local: `local` keeps its value, as in dash, while `typeset` starts it
/// unset, as in zsh and bash. `-p`, or no names with `typeset`, prints the
/// variables as `typeset` commands.
fn declare(sh: &mut Shell, argv: &[Vec<u8>], keep: bool) -> ExecResult {
    let cmd = &argv[0];
    let (mut attrs, mut global, mut print) = (Attrs::default(), false, false);
    // `-f` (definitions) or `+f` (names): functions rather than variables.
    let mut funcs = None;
    let mut i = 1;
    while let Some(a) = argv.get(i) {
        let on = match a.first() {
            Some(b'-') => true,
            Some(b'+') => false,
            _ => break,
        };
        if a.len() < 2 {
            break;
        }
        i += 1;
        if a == b"--" {
            break;
        }
        for &c in &a[1..] {
            match c {
                b'a' if on => (attrs.array, attrs.assoc) = (true, false),
                b'f' if !keep => funcs = Some(on),
                b'A' if on => (attrs.array, attrs.assoc) = (false, true),
                b'g' if on && !keep => global = true,
                b'i' => attrs.integer = Some(on),
                b'l' => attrs.lower = Some(on),
                b'p' if on => print = true,
                b'r' => attrs.readonly = Some(on),
                b'u' => attrs.upper = Some(on),
                b'U' => attrs.unique = Some(on),
                b'x' => attrs.export = Some(on),
                _ => {
                    sh.berr(cmd, format!("Illegal option {}{}", a[0] as char, c as char));
                    // `local` is a special built-in.
                    return if keep { Err(Flow::Error(2)) } else { Ok(2) };
                }
            }
        }
    }
    let args = &argv[i..];
    if let Some(defs) = funcs {
        return Ok(print_functions(sh, cmd, args, defs, &attrs));
    }
    if print || (args.is_empty() && !keep) {
        return Ok(print_declarations(sh, cmd, args, &attrs));
    }
    let local = !global && !sh.locals.is_empty();
    // Without attributes (as for most `local`s) only the assignment is left.
    let plain = attrs.is_empty();
    let mut status = 0;
    for a in args {
        let (name, value) = split_arg(a);
        if name == b"-" && keep {
            // As in dash: the options are restored when the function returns.
            let depth = sh.locals.len();
            if sh.local_options.last().is_none_or(|s| s.0 != depth) {
                sh.local_options.push((depth, sh.options.clone()));
            }
            continue;
        }
        if !is_valid_name(name) {
            return Err(bad_name(sh, cmd, name));
        }
        if local && !sh.locals.last().unwrap().iter().any(|(n, _)| n == name) {
            let old = sh.save_var(name);
            sh.locals.last_mut().unwrap().push((name.to_vec(), old));
            if !keep {
                sh.restore_var(name.to_vec(), None);
                if sh.vartrace.is_some() {
                    sh.trace_unset(name);
                }
            } else if sh.vars.transform(name).any() {
                // `local` keeps the value, but not `-i`, `-l`, `-u` or `-U`
                // (zsh and bash keep neither).
                sh.vars.set_transform(name, Default::default());
            }
            // A local `path` or `dirstack` is an ordinary variable (unset,
            // as in dash), which doesn't change `PATH` (zsh makes `PATH`
            // local too) or the directory stack.
            if let Some(s) = sh.vars.special(name).filter(|s| s.is_tied()) {
                sh.vars.deactivate(s);
            }
        }
        if plain {
            match value {
                Some(v) => assign_arg(sh, name, v)?,
                None => {
                    sh.vars.entry(name);
                }
            }
            continue;
        }
        // A string becomes an array of one element, or an associative
        // array with the key `0` (bash; zsh empties it). An array can't
        // become the other kind (bash; zsh empties it).
        let old = if attrs.array || attrs.assoc {
            sh.vars.get_value(name)
        } else {
            None
        };
        // `path` and `dirstack` (tied arrays) are already arrays.
        let tied = sh.vars.special(name).is_some_and(Special::is_tied);
        let converted = match old {
            _ if tied && attrs.assoc => Err("indexed to associative"),
            _ if tied => Ok(None),
            Some(Value::Array(_)) if attrs.assoc => Err("indexed to associative"),
            Some(Value::Assoc(_)) if attrs.array => Err("associative to indexed"),
            Some(Value::Str(s)) if attrs.assoc => {
                let mut h = crate::vars::Assoc::default();
                h.insert(b"0", s.clone());
                Ok(Some(Value::Assoc(Box::new(h))))
            }
            None if attrs.assoc => Ok(Some(Value::Assoc(Box::default()))),
            Some(Value::Str(_)) | None if attrs.array => Ok(Some(Value::Array(Box::new(
                old.map(|v| v.elements().to_vec()).unwrap_or_default(),
            )))),
            _ => Ok(None),
        };
        match converted {
            Err(how) => {
                let name = String::from_utf8_lossy(name);
                sh.berr(cmd, format!("{name}: cannot convert {how} array"));
                status = 1;
                continue;
            }
            Ok(Some(v)) => sh.set_var_value(name, v)?,
            Ok(None) => {}
        }
        // Set first, so that the value is converted. A value that the
        // variable already has is converted too, as in zsh (bash keeps it).
        let old = sh.vars.transform(name);
        let t = attrs.transform(old);
        if t != old {
            sh.vars.set_transform(name, t);
            let current = match tied {
                true => sh.special_elements(name).map(|a| Value::Array(Box::new(a))),
                false => sh.vars.get_value(name).cloned(),
            };
            if t.any()
                && value.is_none()
                && let Some(v) = current
            {
                sh.set_var_value(name, v)?;
            }
        }
        let value = match value {
            // `typeset -a a=x` is `a=(x)`, `typeset -A h=x` `h=([0]=x)`.
            Some(AssignValue::Str(s)) if attrs.array || attrs.assoc => Some(AssignValue::Items(vec![Item {
                key: attrs.assoc.then(|| b"0".to_vec()),
                value: s,
            }])),
            v => v,
        };
        match value {
            Some(v) => assign_arg(sh, name, v)?,
            None => {
                sh.vars.entry(name);
            }
        }
        let var = sh.vars.entry(name);
        if let Some(on) = attrs.export {
            var.exported = on;
        }
        match attrs.readonly {
            Some(true) => var.readonly = true,
            Some(false) if var.readonly => {
                sh.berr(cmd, format!("{}: is read only", String::from_utf8_lossy(name)));
                return Err(Flow::Error(2));
            }
            _ => {}
        }
    }
    Ok(status)
}

/// `typeset -f` (`defs`) prints the definitions of the functions `names`
/// (all of them without names), `typeset +f` only their names. A name that
/// isn't a function gives status 1, silently (zsh and bash).
fn print_functions(sh: &Shell, cmd: &[u8], names: &[Vec<u8>], defs: bool, attrs: &Attrs) -> i32 {
    let a = attrs;
    let flags = [a.integer, a.lower, a.upper, a.unique, a.readonly, a.export];
    if a.array || a.assoc || flags.iter().any(Option::is_some) {
        sh.berr(cmd, "-f can't be used with variable attributes");
        return 2;
    }
    if names.iter().any(|n| n.contains(&b'=')) {
        sh.berr(cmd, "can't use -f to make functions");
        return 1;
    }
    let mut all: Vec<&Vec<u8>> = sh.functions.keys().collect();
    all.sort_unstable();
    let names = if names.is_empty() { all } else { names.iter().collect() };
    let mut out = Vec::new();
    let mut status = 0;
    for name in names {
        match sh.functions.get(name) {
            Some(f) if defs => out.extend(crate::unparse::function(name, &f.body, &sh.aliases).0),
            Some(_) => {
                out.extend_from_slice(name);
                out.push(b'\n');
            }
            None => status = 1,
        }
    }
    sh.out(&out);
    status
}

/// `typeset -p`: prints the variables `names`, or without names all those
/// with the attributes in `attrs`, as `typeset` commands.
fn print_declarations(sh: &Shell, cmd: &[u8], names: &[Vec<u8>], attrs: &Attrs) -> i32 {
    let mut out = Vec::new();
    let mut status = 0;
    if names.is_empty() {
        // The specials are listed only if they have attributes (as for
        // `set`, which doesn't list them).
        for (name, var) in sh.vars.sorted() {
            let special = var.value.is_none().then(|| sh.special_var(name)).flatten();
            let var = special.as_ref().unwrap_or(var);
            let is_array = matches!(var.value, Some(Value::Array(_)));
            let is_assoc = matches!(var.value, Some(Value::Assoc(_)));
            if (!attrs.array || is_array)
                && (!attrs.assoc || is_assoc)
                && attrs.integer.is_none_or(|i| i == var.transform.integer)
                && attrs.lower.is_none_or(|l| l == var.transform.lower)
                && attrs.upper.is_none_or(|u| u == var.transform.upper)
                && attrs.unique.is_none_or(|u| u == var.transform.unique)
                && attrs.readonly.is_none_or(|r| r == var.readonly)
                && attrs.export.is_none_or(|x| x == var.exported)
            {
                declaration(&mut out, name, var);
            }
        }
    }
    for name in names {
        match sh.special_var(name).or_else(|| sh.vars.take(name)) {
            Some(var) => declaration(&mut out, name, &var),
            None => {
                sh.berr(cmd, format!("no such variable: {}", String::from_utf8_lossy(name)));
                status = 1;
            }
        }
    }
    sh.out(&out);
    status
}

/// A `typeset` command that recreates the variable `name`.
fn declaration(out: &mut Vec<u8>, name: &[u8], var: &crate::vars::Var) {
    out.extend_from_slice(b"typeset ");
    let flags = [
        (matches!(var.value, Some(Value::Array(_))), b'a'),
        (matches!(var.value, Some(Value::Assoc(_))), b'A'),
        (var.transform.integer, b'i'),
        (var.transform.lower, b'l'),
        (var.transform.upper, b'u'),
        (var.transform.unique, b'U'),
        (var.readonly, b'r'),
        (var.exported, b'x'),
    ];
    if flags.iter().any(|f| f.0) {
        out.push(b'-');
        out.extend(flags.iter().filter(|f| f.0).map(|f| f.1));
        out.push(b' ');
    }
    out.extend_from_slice(name);
    if let Some(v) = &var.value {
        out.push(b'=');
        out.extend(quote_value(v));
    }
    out.push(b'\n');
}
