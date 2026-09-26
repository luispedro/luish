//! luish: a POSIX shell.

mod ast;
mod builtins;
mod cmdtext;
mod exec;
mod expand;
mod input;
mod interactive;
mod jobs;
mod lexer;
mod options;
mod parser;
mod path;
mod plugins;
mod prompt;
mod shell;
mod signals;
mod startcache;
mod state;
mod sys;
mod unparse;
mod vars;

use std::ffi::OsStr;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use input::Input;
use options::Opt;
use shell::Shell;
use signals::Disposition;

fn usage_error(sh: &Shell, msg: &str) -> ! {
    sh.error(msg);
    sys::exit(2)
}

fn main() {
    // Rust ignores SIGPIPE; a shell (and its children) must not.
    signals::set_disposition(libc::SIGPIPE, Disposition::Default);

    let args: Vec<Vec<u8>> = std::env::args_os().map(|a| a.into_vec()).collect();
    let mut sh = Shell::new();
    sh.arg0 = args.first().cloned().unwrap_or_else(|| b"luish".to_vec());
    let mut login = sh.arg0.first() == Some(&b'-');

    // Option letters that only make sense on the command line.
    let mut command_mode = false;
    let mut stdin_mode = false;
    let mut force_interactive = false;
    let mut monitor_given = false;
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == b"--" || a == b"-" {
            i += 1;
            break;
        }
        if a.starts_with(b"--") {
            if a != b"--no-plugins" {
                // dash's wording: the second `-` is the illegal letter.
                usage_error(&sh, "Illegal option --");
            }
            sh.no_plugins = true;
            i += 1;
            continue;
        }
        if a.len() < 2 || (a[0] != b'-' && a[0] != b'+') {
            break;
        }
        let on = a[0] == b'-';
        for &c in &a[1..] {
            match c {
                // As in dash, `+c` and `+l` work like `-c` and `-l`.
                b'c' => command_mode = true,
                b'l' => login = true,
                b's' => stdin_mode = on,
                b'i' => force_interactive = on,
                b'o' => {
                    i += 1;
                    let Some(name) = args.get(i) else {
                        usage_error(&sh, "-o requires an argument");
                    };
                    match options::Options::by_name(name) {
                        Some(o) => {
                            monitor_given |= o == Opt::Monitor;
                            sh.options.set(o, on)
                        }
                        None => usage_error(&sh, &format!("Illegal option -o {}", String::from_utf8_lossy(name))),
                    }
                }
                _ => match options::Options::by_letter(c) {
                    Some(o) => {
                        monitor_given |= o == Opt::Monitor;
                        sh.options.set(o, on)
                    }
                    None => usage_error(&sh, &format!("Illegal option {}{}", a[0] as char, c as char)),
                },
            }
        }
        i += 1;
    }
    let operands = &args[i..];

    let mut input;
    if command_mode {
        let Some(cmd) = operands.first() else {
            usage_error(&sh, "-c requires an argument");
        };
        if let Some(a0) = operands.get(1) {
            sh.arg0 = a0.clone();
        }
        sh.positional = operands.get(2..).unwrap_or_default().to_vec();
        input = Input::Whole(Some(cmd.clone()), true);
    } else if stdin_mode || operands.is_empty() {
        sh.positional = operands.to_vec();
        stdin_mode = true;
        input = Input::fd(0, false);
    } else {
        let script = &operands[0];
        sh.arg0 = script.clone();
        sh.positional = operands[1..].to_vec();
        match std::fs::read(OsStr::from_bytes(script)) {
            Ok(text) => input = Input::Whole(Some(text), false),
            Err(e) => {
                sh.lineno = 0;
                let msg = match e.raw_os_error() {
                    Some(libc::ENOENT) => "No such file".to_string(),
                    Some(n) => sys::strerror(n),
                    None => e.to_string(),
                };
                sh.error(format!("cannot open {}: {msg}", String::from_utf8_lossy(script)));
                sys::exit(if e.raw_os_error() == Some(libc::ENOENT) { 127 } else { 2 });
            }
        }
    }

    let interactive = force_interactive || (stdin_mode && !command_mode && sys::isatty(0) && sys::isatty(2));
    if interactive {
        sh.interactive = true;
        sh.options.set(Opt::Interactive, true);
        if !sh.opt(Opt::Vi) {
            sh.options.set(Opt::Emacs, true);
        }
        if !monitor_given {
            sh.options.set(Opt::Monitor, true);
        }
        for sig in [libc::SIGINT, libc::SIGQUIT, libc::SIGTERM] {
            if !sh.ignored_on_entry[sig as usize] {
                signals::set_disposition(sig, sh.default_disposition(sig));
            }
        }
        sh.set_jobctl(sh.opt(Opt::Monitor));
        // With -c or a script, the interactive shell still runs that.
        if stdin_mode {
            if sys::isatty(0) && interactive::init_editor(&sh) {
                input = Input::Editor;
            } else {
                input = Input::fd(0, true);
            }
        }
    }
    if interactive {
        interactive::rc_d(&mut sh);
    }
    if login {
        interactive::login_profiles(&mut sh);
    }
    if interactive {
        interactive::startup(&mut sh);
    }
    if stdin_mode {
        sh.options.set(Opt::Stdin, true);
    }
    sh.set_jobctl(sh.opt(Opt::Monitor));

    sh.run_input(&mut input);
    let status = sh.last_status;
    sh.exit(status);
}
