//! `style`: shows and changes styles and colour schemes (`crate::style`).
//! A built-in only in interactive shells; `__luish_internal style` is the
//! same command everywhere (and what the saved state uses).

use crate::shell::{ExecResult, Shell};
use crate::style::{self, Background, Choice, Style, check_name, check_scheme_name, terminal_key, terminal_text};

/// The command that the saved state uses.
pub const STATE_COMMAND: &str = "__luish_internal style";

pub fn style(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// The background, and what told it: `$LUISH_BACKGROUND` (as the user set
/// it, or as the shell did from `$COLORFGBG` or the terminal), then
/// `$COLORFGBG`.
pub fn background(sh: &Shell) -> (Option<Background>, &'static str) {
    let luish = sh.get_var(b"LUISH_BACKGROUND");
    if let Some(b) = style::background(luish.as_deref(), None) {
        return match crate::interactive::detected_background() {
            Some((d, source)) if d == b => (Some(b), source),
            _ => (Some(b), "$LUISH_BACKGROUND"),
        };
    }
    match style::background(None, sh.get_var(b"COLORFGBG").as_deref()) {
        Some(b) => (Some(b), "$COLORFGBG"),
        None => (None, ""),
    }
}

/// The scheme in use.
pub fn scheme_in_use(sh: &Shell) -> Option<String> {
    sh.styles.choice().scheme(background(sh).0).map(str::to_owned)
}

fn text(arg: &[u8]) -> String {
    String::from_utf8_lossy(arg).into_owned()
}

pub fn run(sh: &mut Shell, name: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let fail = |sh: &Shell, msg: String| {
        sh.berr(name, msg);
        Ok(1)
    };
    let (first, rest) = match args.split_first() {
        None => return Ok(list(sh)),
        Some((f, rest)) => (f.as_slice(), rest),
    };
    let result = match first {
        b"-p" if rest.is_empty() => {
            let cmd = text(name);
            let out: Vec<u8> = (sh.styles.state(&cmd).into_iter()).flat_map(|e| e.1).collect();
            return Ok(sh.out_status(&out));
        }
        b"-r" => {
            for n in rest {
                sh.styles.remove_user(&text(n));
            }
            Ok(())
        }
        b"--clear" if rest.is_empty() => {
            sh.styles.clear();
            Ok(())
        }
        b"-c" => choose(sh, rest),
        b"--terminal-colors" | b"--terminal-colours" => match rest {
            [] => {
                let on = if sh.styles.terminal_colors() { "on\n" } else { "off\n" };
                return Ok(sh.out_status(on.as_bytes()));
            }
            [v] if v == b"on" || v == b"off" => {
                sh.styles.set_terminal_colors(v == b"on");
                Ok(())
            }
            _ => Err(format!("{}: expected on or off", text(first))),
        },
        b"--detect" if rest.is_empty() => match crate::interactive::ask_background(sh) {
            Some(_) => Ok(()),
            None => Err("the terminal didn't tell its background colour".to_owned()),
        },
        b"-s" => match rest.split_first() {
            Some((scheme, rest)) => return scheme_command(sh, name, &text(scheme), rest),
            None => Err("-s: missing scheme".to_owned()),
        },
        b"-d" => match rest.split_first() {
            Some((r, names)) if r == b"-r" => {
                for n in names {
                    sh.styles.remove_default(&text(n));
                }
                Ok(())
            }
            Some((n, value)) => parse(n, value).map(|(n, v)| sh.styles.set_default(&n, v)),
            None => Err("-d: missing name".to_owned()),
        },
        b"--" => return set_or_show(sh, name, rest),
        [b'-', _, ..] => {
            sh.berr(name, format!("bad option: {}", text(first)));
            return Ok(2);
        }
        _ => return set_or_show(sh, name, args),
    };
    match result {
        Ok(()) => Ok(0),
        Err(msg) => fail(sh, msg),
    }
}

/// A name and its value.
fn parse(name: &[u8], value: &[Vec<u8>]) -> Result<(String, Style), String> {
    let name = text(name);
    check_name(&name)?;
    let v = Style::parse(value).map_err(|e| format!("{name}: {e}"))?;
    Ok((name, v))
}

/// `style NAME [VALUE...]`.
fn set_or_show(sh: &mut Shell, cmd: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let Some((name, value)) = args.split_first() else {
        return Ok(list(sh));
    };
    if !value.is_empty() {
        return match parse(name, value) {
            Ok((n, v)) => {
                sh.styles.set_user(&n, v);
                Ok(0)
            }
            Err(msg) => {
                sh.berr(cmd, msg);
                Ok(1)
            }
        };
    }
    let name = text(name);
    if let Err(msg) = check_name(&name) {
        sh.berr(cmd, msg);
        return Ok(1);
    }
    let scheme = scheme_in_use(sh);
    let r = sh.styles.resolver(scheme.as_deref());
    let mut out = format!("{name} {}\n", r.get(&name).text());
    let mut cur = Some(name.as_str());
    while let Some(n) = cur {
        if let Some((v, origin)) = r.value(n) {
            out.push_str(&format!("  {n}: {} ({origin})\n", v.text()));
            if v.plain {
                break;
            }
        }
        cur = style::parent(n);
    }
    Ok(sh.out_status(out.as_bytes()))
}

/// `style` alone: every name with its value.
fn list(sh: &Shell) -> i32 {
    let scheme = scheme_in_use(sh);
    let r = sh.styles.resolver(scheme.as_deref());
    let mut out = String::new();
    for n in sh.styles.names() {
        out.push_str(&format!("{n:<20} {}\n", r.get(&n).text()));
    }
    sh.out_status(out.as_bytes())
}

/// `style -c [NAME | DARK LIGHT [DEFAULT]]`.
fn choose(sh: &mut Shell, args: &[Vec<u8>]) -> Result<(), String> {
    let names: Vec<String> = args.iter().map(|a| text(a)).collect();
    for n in &names {
        check_scheme_name(n)?;
        if !sh.styles.has_scheme(n) {
            return Err(format!("no such colour scheme: {n}"));
        }
    }
    let c = match &names[..] {
        [] => {
            sh.out(list_schemes(sh).as_bytes());
            return Ok(());
        }
        [one] => Choice::One(one.clone()),
        [dark, light] | [dark, light, _] => Choice::Pair {
            dark: dark.clone(),
            light: light.clone(),
            default: names.get(2).cloned(),
        },
        _ => return Err("-c: too many arguments".to_owned()),
    };
    sh.styles.set_choice(c);
    Ok(())
}

/// The schemes, with the one in use marked, and why.
fn list_schemes(sh: &Shell) -> String {
    let (bg, source) = background(sh);
    let choice = sh.styles.choice();
    let in_use = choice.scheme(bg);
    let mut names: Vec<&str> = sh.styles.scheme_names();
    for n in choice.names() {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let mut out = String::new();
    for n in names {
        let mark = if in_use == Some(n) { '*' } else { ' ' };
        out.push_str(&format!("{mark} {n}"));
        if !sh.styles.has_scheme(n) {
            out.push_str(" (not defined)");
        } else if let Some(m) = sh.styles.missing(Some(n)) {
            out.push_str(&format!(" (inherits {m}, which is not defined)"));
        }
        if in_use == Some(n) && matches!(choice, Choice::Pair { .. }) {
            match bg {
                Some(Background::Dark) => out.push_str(&format!(" (dark background, from {source})")),
                Some(Background::Light) => out.push_str(&format!(" (light background, from {source})")),
                None => out.push_str(" (background unknown)"),
            }
        }
        out.push('\n');
    }
    out
}

/// `style -s SCHEME ...`.
fn scheme_command(sh: &mut Shell, cmd: &[u8], scheme: &str, args: &[Vec<u8>]) -> ExecResult {
    let result = (|| {
        check_scheme_name(scheme)?;
        match args.split_first() {
            None => {
                let s = sh
                    .styles
                    .scheme(scheme)
                    .ok_or(format!("no such colour scheme: {scheme}"))?;
                let mut out = String::new();
                if let Some(p) = &s.inherits {
                    out.push_str(&format!("(inherits {p})\n"));
                }
                for (n, v) in &s.values {
                    out.push_str(&format!("{n:<20} {}\n", v.text()));
                }
                for (k, v) in &s.terminal {
                    let n = format!("{}{k}", style::TERMINAL_PREFIX);
                    out.push_str(&format!("{n:<20} {}\n", terminal_text(v)));
                }
                sh.out(out.as_bytes());
                Ok(())
            }
            Some((a, rest)) if a == b"-i" => match rest {
                [p] if p.is_empty() => sh.styles.set_inherits(scheme, None),
                [p] => {
                    let p = text(p);
                    check_scheme_name(&p)?;
                    sh.styles.set_inherits(scheme, Some(&p))
                }
                _ => Err("-i: expected one scheme".to_owned()),
            },
            Some((a, [])) if a == b"--delete" => match sh.styles.delete_scheme(scheme) {
                true => Ok(()),
                false => Err(format!("no colour scheme of the user's or a plugin's: {scheme}")),
            },
            Some((a, names)) if a == b"-r" => {
                for n in names {
                    sh.styles.remove_from_scheme(scheme, &text(n));
                }
                Ok(())
            }
            Some((n, [])) if terminal_key(&text(n)).is_some() => {
                let s = sh
                    .styles
                    .scheme(scheme)
                    .ok_or(format!("no such colour scheme: {scheme}"))?;
                let n = text(n);
                let key = terminal_key(&n).unwrap_or_default();
                style::check_terminal_key(key).map_err(|e| format!("{n}: {e}"))?;
                let v = s.terminal.get(key).map_or("(not set)".to_owned(), |v| terminal_text(v));
                sh.out(format!("{n} {v}\n").as_bytes());
                Ok(())
            }
            Some((n, value)) if terminal_key(&text(n)).is_some() => {
                let n = text(n);
                let key = terminal_key(&n).unwrap_or_default();
                let colors = style::parse_terminal(key, value).map_err(|e| format!("{n}: {e}"))?;
                sh.styles.set_terminal_in_scheme(scheme, key, colors);
                Ok(())
            }
            Some((n, [])) => {
                let s = sh
                    .styles
                    .scheme(scheme)
                    .ok_or(format!("no such colour scheme: {scheme}"))?;
                let n = text(n);
                check_name(&n)?;
                let v = s.values.get(&n).map_or("(not set)".to_owned(), Style::text);
                sh.out(format!("{n} {v}\n").as_bytes());
                Ok(())
            }
            Some((n, value)) => {
                let (n, v) = parse(n, value)?;
                sh.styles.set_in_scheme(scheme, &n, v);
                Ok(())
            }
        }
    })();
    match result {
        Ok(()) => Ok(0),
        Err(msg) => {
            sh.berr(cmd, msg);
            Ok(1)
        }
    }
}
