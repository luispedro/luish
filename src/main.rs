//! luish: a POSIX shell.

// The C `main` below replaces Rust's runtime start-up, which would cost
// a shell started for each command a dozen syscalls: it reads
// /proc/self/maps and sets up an alternate stack to report stack overflows,
// polls fds 0 to 2 to reopen them on /dev/null if closed (dash leaves them
// closed), and ignores SIGPIPE (which a shell must leave as it found it).
// The arguments are taken from `argv` rather than `std::env::args`, which
// without it would be empty on some libcs (musl).
#![cfg_attr(not(test), no_main)]

mod ast;
mod builtins;
mod cmdtext;
mod config;
mod exec;
mod expand;
mod frames;
mod hash;
mod input;
mod interactive;
mod jobs;
mod lexer;
mod options;
mod parser;
mod path;
mod plugins;
mod prompt;
mod remote;
mod shell;
mod signals;
mod stack;
mod startcache;
mod state;
mod style;
mod sys;
mod unparse;
mod vars;
mod vartrace;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use input::Input;
use options::Opt;
use shell::Shell;

fn usage_error(sh: &Shell, msg: &str) -> ! {
    sh.error(msg);
    sys::exit(2)
}

const USAGE: &str = "\
Usage: luish [OPTION...] [SCRIPT [ARG...]]
       luish [OPTION...] -c COMMAND [ARG0 [ARG...]]
       luish [OPTION...] -s [ARG...]

Without SCRIPT or -c, luish reads commands from standard input, as an
interactive shell if it is a terminal.

  -c                  run COMMAND, with ARG0 as $0 and ARGs as $1, $2...
  -s, --stdin         read commands from standard input (ARGs are $1, $2...)
  -i, --interactive   run as an interactive shell
  -l, --login         run as a login shell: read /etc/profile and ~/.profile
                      (or ~/.config/luish/login.d/)
  -LETTER, +LETTER    set or unset an option by its letter, as with `set`
  -o NAME, +o NAME    set or unset any option, named as for `setopt`: case
                      and `_` don't matter, and a `no` prefix inverts it
                      (-o err_exit, -o no_glob, -o prompt_percent)
  --no-rcs            don't read any startup files
  --no-plugins        make `plugin load` do nothing
  --remote CMD...     edit lines here and run them in the shell that CMD
                      starts with `luish --serve`, usually through ssh
  --ssh [OPTION...] HOST
                      the same as --remote ssh -T [OPTION...] HOST luish --serve,
                      with a copy of this luish that it keeps on HOST; among
                      the OPTIONs, luish's own:
      --luish-path=PROGRAM  run PROGRAM on HOST instead of a copy
      -e, --rsh=COMMAND, --ssh-command=COMMAND
                            COMMAND instead of ssh -T (split into words)
      --copy-luish          copy luish to HOST again, even if it is there
      -o ssh.no_auto_copy   never copy luish (use the copy, or HOST's luish)
      -o ssh.escape_char=C  C instead of ~ for the escapes (~. ~^Z ~? ~~),
                            or none
  --help              show this help and exit
  --version           show the version and exit

In an interactive shell, `help` lists the built-in commands.
";

/// What the command line's options choose, apart from the shell options.
#[derive(Default)]
struct Invocation {
    /// `-c`.
    command: bool,
    /// `-s`.
    stdin: bool,
    /// `-i`.
    interactive: bool,
    /// `-l`, or `$0` starting with `-`.
    login: bool,
    /// `-m` (or `+m`) was given, so an interactive shell keeps it.
    monitor_given: bool,
    /// `--no-rcs`.
    no_rcs: bool,
}

impl Invocation {
    /// Sets an option given on the command line. As in dash, `interactive`
    /// and `stdin` (by letter or with `-o`) choose how the shell runs.
    fn set(&mut self, sh: &mut Shell, o: Opt, on: bool) {
        match o {
            Opt::Interactive => self.interactive = on,
            Opt::Stdin => self.stdin = on,
            _ => {
                self.monitor_given |= o == Opt::Monitor;
                sh.options.set(o, on);
            }
        }
    }

    /// Handles luish's long options.
    fn long_option(&mut self, sh: &mut Shell, arg: &[u8]) {
        // For `__luish_internal check-cache` (`startcache.rs`).
        if let Some(v) = arg.strip_prefix(b"--internal-check-cache=")
            && let Some(colon) = v.iter().position(|&c| c == b':')
        {
            sh.check_cache = Some((v[..colon].to_vec(), v[colon + 1..].to_vec()));
            return;
        }
        match arg {
            b"--help" => {
                sys::write_all(1, USAGE.as_bytes());
                sys::exit(0)
            }
            b"--version" => {
                let v = format!(
                    "luish {} ({})\n",
                    env!("CARGO_PKG_VERSION"),
                    builtins::internal::GIT_REV_SHORT
                );
                sys::write_all(1, v.as_bytes());
                sys::exit(0)
            }
            b"--login" => self.login = true,
            b"--interactive" => self.interactive = true,
            b"--stdin" => self.stdin = true,
            b"--no-rcs" => self.no_rcs = true,
            b"--no-plugins" => sh.no_plugins = true,
            _ => usage_error(sh, &format!("Illegal option {}", String::from_utf8_lossy(arg))),
        }
    }
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
extern "C" fn main(argc: libc::c_int, argv: *const *const libc::c_char) -> libc::c_int {
    stack::init();
    let args = (0..argc.max(0) as usize)
        // SAFETY: the C runtime passes `argc` valid NUL-terminated strings.
        .map(|i| unsafe { std::ffi::CStr::from_ptr(*argv.add(i)) }.to_bytes().to_vec())
        .collect();
    run(args)
}

#[cfg_attr(test, allow(dead_code))]
fn run(mut args: Vec<Vec<u8>>) -> ! {
    // The SSH mode: the client, or the server, which then runs as an
    // interactive shell on its own pty (`remote/`).
    match args.get(1).map(Vec::as_slice) {
        Some(b"--remote") => remote::client::run(&args[2..]),
        Some(b"--ssh") => remote::client::ssh(&args[2..]),
        _ => {}
    }
    let serve = (args.get(1).is_some_and(|a| a == b"--serve")).then(|| {
        args[1] = b"-i".to_vec();
        remote::relay::start()
    });
    let mut sh = Shell::new();
    sh.arg0 = args.first().cloned().unwrap_or_else(|| b"luish".to_vec());
    let mut inv = Invocation {
        login: sh.arg0.first() == Some(&b'-'),
        ..Default::default()
    };

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == b"--" || a == b"-" {
            i += 1;
            break;
        }
        if a.starts_with(b"--") {
            inv.long_option(&mut sh, a);
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
                b'c' => inv.command = true,
                b'l' => inv.login = true,
                b'o' => {
                    i += 1;
                    let Some(name) = args.get(i) else {
                        usage_error(&sh, "-o requires an argument");
                    };
                    // Any option, named as for `setopt` (so `+o noglob`
                    // and `-o glob` both turn globbing on).
                    match options::Options::find(name) {
                        Some(options::Setting::Flag(o, named_on)) => inv.set(&mut sh, o, on == named_on),
                        _ => usage_error(&sh, &format!("Illegal option -o {}", String::from_utf8_lossy(name))),
                    }
                }
                _ => match options::Options::by_letter(c) {
                    Some(o) => inv.set(&mut sh, o, on),
                    None => usage_error(&sh, &format!("Illegal option {}{}", a[0] as char, c as char)),
                },
            }
        }
        i += 1;
    }
    let operands = &args[i..];

    let Invocation {
        command: command_mode,
        stdin: mut stdin_mode,
        interactive: force_interactive,
        login,
        monitor_given,
        no_rcs,
    } = inv;

    let mut input;
    if command_mode {
        let Some(cmd) = operands.first() else {
            usage_error(&sh, "-c requires an argument");
        };
        if let Some(a0) = operands.get(1) {
            sh.arg0 = a0.clone();
        }
        sh.command_arg = Some(i);
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
    sh.bump_shlvl(interactive);
    // `-o vars.trace`: from the start, so that what startup files set is
    // recorded (`vartrace.rs`).
    sh.update_var_trace(true);
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
            if !signals::ignored_on_entry(sig as usize) {
                signals::set_disposition(sig, sh.default_disposition(sig));
            }
        }
        sh.set_jobctl(sh.opt(Opt::Monitor));
        // With -c or a script, the interactive shell still runs that.
        if stdin_mode {
            if sys::isatty(0) && interactive::init_editor() {
                input = Input::Editor;
            } else {
                input = Input::fd(0, true);
            }
        }
    }
    if let Some(fd) = serve {
        interactive::remote::start_serving(fd);
    }
    if !no_rcs {
        if matches!(input, Input::Editor) && sys::isatty(2) && !sh.no_plugins {
            interactive::firstrun::run(&mut sh);
        }
        if interactive {
            interactive::rc_d(&mut sh);
        } else if login {
            config::load_login(&mut sh);
        }
        if login {
            interactive::login_profiles(&mut sh);
        }
        if interactive {
            interactive::startup(&mut sh);
        }
        if sh.check_cache.is_some() {
            startcache::check_not_reached(&mut sh);
        }
    }
    if interactive {
        interactive::load_history(&sh);
    }
    if stdin_mode {
        sh.options.set(Opt::Stdin, true);
    }
    sh.set_jobctl(sh.opt(Opt::Monitor));

    if !command_mode && !stdin_mode {
        let file = frames::SourceFile::new(&operands[0], sh.curdir.as_deref());
        sh.push_frame(frames::Frame::file(frames::FrameKind::Script, file, 0));
    }
    sh.run_input(&mut input);
    let status = sh.last_status;
    sh.exit(status);
}
