//! The client of the SSH mode (`luish --remote CMD...`, `luish --ssh
//! HOST`): runs CMD, which starts `luish --serve` at the other end, reads
//! the lines the server asks for with the local line editor, and in between
//! passes the terminal through to the server's pty, as ssh does.
//!
//! A thread reads what the server sends: the pty's output is written to
//! the terminal as it comes, and the other messages go to the main thread,
//! which a byte on a pipe wakes while it waits for the terminal.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use super::escape::{self, Action, Escapes};
use super::{COPY, Dec, Enc, MAGIC, Reader, VERSION, msg, write_frame};
use crate::builtins::internal::BUILD_ID;
use crate::interactive::remote::{self as editor, Link};
use crate::sys;

/// A message from the server, or None when the connection has gone.
type Event = Option<(u8, Vec<u8>)>;

/// The write end of the pipe that wakes the main thread.
static WAKE: AtomicI32 = AtomicI32::new(-1);
/// Set when the window size changes.
static RESIZED: AtomicBool = AtomicBool::new(false);

fn fail(what: &str) -> ! {
    let msg = format!("luish: {what}\n");
    sys::write_all(2, msg.as_bytes());
    sys::exit(255)
}

/// `luish --ssh [OPTION...] HOST`: `ssh -T [OPTION...] HOST` with a script
/// that runs a copy of this luish, kept on the server by version and
/// contents, after copying it there if it isn't (`Image`). See `Ssh::parse`
/// for luish's own options among ssh's.
pub fn ssh(args: &[Vec<u8>]) -> ! {
    let Ssh {
        mut cmd,
        program,
        copy,
        escape,
    } = Ssh::parse(args).unwrap_or_else(|e| fail(&e));
    let image = program.is_none().then(Image::new);
    match &image {
        Some(image) => cmd.push(image.script(copy)),
        None => cmd.extend([program.unwrap_or_else(|| b"luish".to_vec()), b"--serve".to_vec()]),
    }
    connect(&cmd, image, escape)
}

/// When `--ssh` copies luish to the server.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CopyMode {
    /// When the server has no copy of this build (the default).
    Auto,
    /// Never (`-o ssh.no_auto_copy`): it runs the copy if it is there, else
    /// the `luish` on its `PATH`.
    Never,
    /// Always, replacing the copy that is there (`--copy-luish`).
    Always,
}

/// What `--ssh`'s arguments say.
#[derive(Debug, PartialEq)]
struct Ssh {
    /// The command, up to the one for the server.
    cmd: Vec<Vec<u8>>,
    /// `--luish-path`.
    program: Option<Vec<u8>>,
    copy: CopyMode,
    /// The escape character (`-o ssh.escape_char`), None for none.
    escape: Option<u8>,
}

/// An option given as `-o ssh.NAME`.
#[derive(Debug, PartialEq)]
enum SshOption {
    Copy(CopyMode),
    Escape(Option<u8>),
}

impl Ssh {
    /// luish's own options, anywhere among ssh's (which never start with
    /// `--`, and have no `.` in the names given to `-o`):
    /// - `--luish-path=PROGRAM` runs `PROGRAM --serve` instead of a copy, as
    ///   rsync's `--rsync-path`: the remote shell runs it, so `~/bin/luish`
    ///   works;
    /// - `-e COMMAND`, `--rsh=COMMAND` or `--ssh-command=COMMAND` replaces
    ///   `ssh -T`, as rsync's `--rsh`:
    ///   split into words at blanks, with quotes as in the shell;
    /// - `--copy-luish` and `-o ssh.no_auto_copy` (`CopyMode`);
    /// - `-o ssh.escape_char=C` (`escape.rs`).
    ///
    /// ssh's own `-e` (its escape character) would do nothing: ssh's input
    /// is a pipe, and the client has the escapes instead.
    fn parse(args: &[Vec<u8>]) -> Result<Ssh, String> {
        let mut program = None;
        let mut rsh = None;
        let mut copy = CopyMode::Auto;
        let mut escape = Some(b'~');
        let mut copy_luish = false;
        let mut rest = Vec::new();
        let mut args = args.iter();
        while let Some(a) = args.next() {
            // The value of an option that takes one, given as `--NAME=VALUE`,
            // `--NAME VALUE`, `-xVALUE` or `-x VALUE`.
            let mut value = |long: Option<&str>, short: Option<&str>| -> Result<Option<Vec<u8>>, String> {
                if let Some(v) = long.and_then(|l| a.strip_prefix(format!("{l}=").as_bytes())) {
                    return Ok(Some(v.to_vec()));
                }
                if let Some(v) = short.and_then(|s| a.strip_prefix(s.as_bytes()))
                    && !v.is_empty()
                {
                    return Ok(Some(v.to_vec()));
                }
                let Some(name) = [long, short].into_iter().flatten().find(|n| a == n.as_bytes()) else {
                    return Ok(None);
                };
                args.next()
                    .cloned()
                    .map(Some)
                    .ok_or_else(|| format!("{name} requires an argument"))
            };
            if let Some(p) = value(Some("--luish-path"), None)? {
                program = Some(p);
            } else if let Some(c) = value(Some("--rsh"), Some("-e"))? {
                rsh = Some(c);
            } else if let Some(c) = value(Some("--ssh-command"), None)? {
                rsh = Some(c);
            } else if a == b"--copy-luish" {
                copy_luish = true;
            } else if let Some(o) = value(None, Some("-o"))? {
                match o.strip_prefix(b"ssh.") {
                    Some(name) => match Self::option(name)? {
                        SshOption::Copy(c) => copy = c,
                        SshOption::Escape(e) => escape = e,
                    },
                    None => rest.extend([b"-o".to_vec(), o]),
                }
            } else {
                rest.push(a.clone());
            }
        }
        if rest.is_empty() {
            return Err("--ssh requires a host".into());
        }
        if program.as_ref().is_some_and(|p| p.is_empty()) {
            return Err("--luish-path requires a program".into());
        }
        if copy_luish {
            if program.is_some() {
                return Err("--copy-luish and --luish-path can't be used together".into());
            }
            copy = CopyMode::Always;
        }
        let mut cmd = match rsh {
            Some(c) => split_words(&c).ok_or("--rsh: unmatched quote")?,
            None => vec![b"ssh".to_vec(), b"-T".to_vec()],
        };
        if cmd.is_empty() {
            return Err("--rsh requires a command".into());
        }
        cmd.append(&mut rest);
        Ok(Ssh {
            cmd,
            program,
            copy,
            escape,
        })
    }

    /// `-o ssh.NAME` or `-o ssh.NAME=VALUE`, whose NAME is matched as
    /// `setopt`'s: case and `_` don't matter, and a `no` prefix inverts it.
    fn option(option: &[u8]) -> Result<SshOption, String> {
        let (name, value) = match option.iter().position(|&c| c == b'=') {
            Some(i) => (&option[..i], Some(&option[i + 1..])),
            None => (option, None),
        };
        let norm: Vec<u8> = name
            .iter()
            .filter(|&&c| c != b'_')
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match (&norm[..], value) {
            (b"autocopy", None) => Ok(SshOption::Copy(CopyMode::Auto)),
            (b"noautocopy", None) => Ok(SshOption::Copy(CopyMode::Never)),
            // A printable character, or none (ssh also takes `^X`, which the
            // line editor would read as a key of its own).
            (b"escapechar", Some(b"none")) => Ok(SshOption::Escape(None)),
            (b"escapechar", Some(&[c])) if c.is_ascii_graphic() => Ok(SshOption::Escape(Some(c))),
            (b"escapechar", _) => Err("--ssh: ssh.escape_char must be a printable character or none".into()),
            _ => Err(format!("--ssh: unknown option ssh.{}", String::from_utf8_lossy(name))),
        }
    }
}

/// Splits `--rsh`'s COMMAND into words at blanks, with `'...'`, `"..."`
/// and `\` quoting as in the shell. None for an unmatched quote.
fn split_words(s: &[u8]) -> Option<Vec<Vec<u8>>> {
    let mut words = Vec::new();
    let mut word: Option<Vec<u8>> = None;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        i += 1;
        match c {
            b' ' | b'\t' | b'\n' => words.extend(word.take()),
            b'\'' => {
                let end = i + s[i..].iter().position(|&c| c == b'\'')?;
                word.get_or_insert_default().extend_from_slice(&s[i..end]);
                i = end + 1;
            }
            b'"' => {
                let w = word.get_or_insert_default();
                loop {
                    match *s.get(i)? {
                        b'"' => break,
                        b'\\' if matches!(s.get(i + 1), Some(b'"' | b'\\' | b'$' | b'`')) => {
                            w.push(s[i + 1]);
                            i += 1;
                        }
                        c => w.push(c),
                    }
                    i += 1;
                }
                i += 1;
            }
            b'\\' => {
                let w = word.get_or_insert_default();
                if let Some(&c) = s.get(i) {
                    w.push(c);
                    i += 1;
                }
            }
            c => word.get_or_insert_default().push(c),
        }
    }
    words.extend(word);
    Some(words)
}

/// This luish's executable, for `--ssh` to copy to the server. It is kept
/// there as `~/.cache/luish/binaries/luish-VERSION-BUILD`, BUILD being
/// `BUILD_ID` (the commit, and a hash of the sources if they differ from
/// it), so that each version (or build) of the client runs its own,
/// whatever else is installed.
struct Image {
    /// The copy's file name.
    name: String,
}

/// The command `--ssh` runs on the server, in `sh -c '...'` for whatever
/// the user's shell is: so on one line, with no `'`, no `!` (for csh) and
/// no `\\` (for fish). It is `SCRIPT_START`, `SCRIPT_RUN` and `SCRIPT_COPY`
/// (the parts as `CopyMode` says), then `SCRIPT_END`.
///
/// It runs the copy (NAME) if it is there; else it writes `COPY` and `uname
/// -sm`, and reads a count of bytes and that many bytes: the copy, which it
/// checks runs (a build linked with a newer libc than the server's
/// doesn't), and removes copies more than 30 days old. With 0 bytes, or a
/// copy that can't run, it runs the `luish` on its `PATH` instead.
const SCRIPT_START: &str = "d=${XDG_CACHE_HOME:-$HOME/.cache}/luish/binaries; f=$d/NAME; ";
const SCRIPT_RUN: &str = r#"if [ -x "$f" ]; then exec "$f" --serve; fi; "#;
const SCRIPT_COPY: &str = concat!(
    r#"printf "\0luish-copy\0%s\n" "$(uname -sm)"; read n; "#,
    r#"case $n in [1-9]*) t=$f.$$; "#,
    r#"mkdir -p "$d" && head -c "$n" >"$t" && [ $(wc -c <"$t") = "$n" ] "#,
    r#"|| { rm -f "$t"; echo "luish: cannot copy luish to $f" >&2; exit 255; }; "#,
    r#"chmod +x "$t"; if "$t" --version >/dev/null; then "#,
    r#"find "$d" -name "luish-*" -mtime +30 -exec rm -f {} +; "#,
    r#"mv -f "$t" "$f"; exec "$f" --serve; fi; "#,
    r#"rm -f "$t"; echo "luish: the copy of luish cannot run on this host" >&2;; "#,
    "esac; ",
);
const SCRIPT_END: &str = "exec luish --serve";

impl Image {
    fn new() -> Image {
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        // Outside a git checkout, `BUILD_ID` starts with the version.
        let id = BUILD_ID.strip_prefix(VERSION).unwrap_or(BUILD_ID);
        let id = id.strip_prefix('-').unwrap_or(id);
        Image {
            name: format!("luish-{VERSION}-{id}"),
        }
    }

    fn script(&self, copy: CopyMode) -> Vec<u8> {
        let (run, fetch) = match copy {
            CopyMode::Auto => (SCRIPT_RUN, SCRIPT_COPY),
            CopyMode::Never => (SCRIPT_RUN, ""),
            CopyMode::Always => ("", SCRIPT_COPY),
        };
        let script = [SCRIPT_START, run, fetch, SCRIPT_END].concat();
        format!("sh -c '{}'", script.replace("NAME", &self.name)).into_bytes()
    }

    /// Answers the script, which has written `uname -sm` (as `host`):
    /// sends the copy if the server is the same system as this one, else
    /// 0. False if the server has gone.
    fn send(&self, to: i32, host: &[u8]) -> bool {
        let here = uname();
        let bytes = if host != here.as_bytes() {
            Err(format!("the server runs {}, not {here}", String::from_utf8_lossy(host)))
        } else {
            std::fs::read("/proc/self/exe").map_err(|e| format!("cannot read /proc/self/exe: {e}"))
        };
        let bytes = match bytes {
            Ok(b) => b,
            Err(why) => {
                sys::write_all(2, format!("luish: {why}: using the server's own luish\n").as_bytes());
                return sys::write_all(to, b"0\n");
            }
        };
        let msg = format!(
            "luish: copying luish {} to the server ({:.1} MB)\n",
            env!("CARGO_PKG_VERSION"),
            bytes.len() as f64 / 1e6
        );
        sys::write_all(2, msg.as_bytes());
        sys::write_all(to, format!("{}\n", bytes.len()).as_bytes()) && sys::write_all(to, &bytes)
    }
}

/// This system, as `uname -sm` writes it.
fn uname() -> String {
    // SAFETY: uname fills the struct, whose fields are NUL-terminated.
    unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut u) != 0 {
            return String::new();
        }
        let field = |f: &[libc::c_char]| std::ffi::CStr::from_ptr(f.as_ptr()).to_string_lossy().into_owned();
        format!("{} {}", field(&u.sysname), field(&u.machine))
    }
}

struct Conn {
    /// The transport's stdin.
    to: i32,
    events: Receiver<Event>,
}

impl Link for Rc<Conn> {
    fn send(&self, kind: u8, data: &[u8]) -> bool {
        write_frame(self.to, kind, data)
    }

    fn reply(&self, wait: Option<Duration>) -> Option<(u8, Vec<u8>)> {
        match wait {
            None => self.events.recv().ok().flatten(),
            Some(w) => self.events.recv_timeout(w).unwrap_or_default(),
        }
    }
}

/// `luish --remote CMD...`.
pub fn run(cmd: &[Vec<u8>]) -> ! {
    connect(cmd, None, Some(b'~'))
}

/// Runs CMD and talks to the server it starts, first copying `image` if
/// the server asks for it, with `escape` as the escape character.
fn connect(cmd: &[Vec<u8>], image: Option<Image>, escape: Option<u8>) -> ! {
    if cmd.is_empty() {
        fail("--remote requires a command");
    }
    if !sys::isatty(0) || !sys::isatty(1) {
        fail("--remote: standard input and output must be a terminal");
    }
    let (to_rd, to_wr) = sys::pipe().unwrap_or_else(|e| fail(&sys::strerror(e)));
    let (from_rd, from_wr) = sys::pipe().unwrap_or_else(|e| fail(&sys::strerror(e)));
    let pid = sys::fork().unwrap_or_else(|e| fail(&format!("fork: {}", sys::strerror(e))));
    if pid == 0 {
        let _ = sys::dup2(to_rd, 0);
        let _ = sys::dup2(from_wr, 1);
        let argv: Vec<_> = cmd.iter().map(|a| sys::cstr(a)).collect();
        let mut p: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
        p.push(std::ptr::null());
        // SAFETY: NUL-terminated strings, in a null-terminated array.
        unsafe { libc::execvp(p[0], p.as_ptr()) };
        fail(&format!(
            "cannot run {}: {}",
            String::from_utf8_lossy(&cmd[0]),
            sys::strerror(sys::errno())
        ));
    }
    sys::close(to_rd);
    sys::close(from_wr);
    // A server that has gone is noticed when writing to it.
    // SAFETY: plain signal call.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };

    let (cols, rows) = super::window_size(1).unwrap_or((80, 24));
    let vars: Vec<_> = std::env::vars_os()
        .filter(|(name, _)| super::forward(name.as_encoded_bytes()).is_some())
        .collect();
    let mut hello = Enc::default();
    hello
        .u32(VERSION)
        .u32(cols.into())
        .u32(rows.into())
        .list(&vars, |e, (name, value)| {
            e.bytes(name.as_encoded_bytes()).bytes(value.as_encoded_bytes());
        });
    let mut from = Reader::default();
    // What the remote startup files printed before luish started.
    loop {
        if let Some(image) = &image
            && let Some(before) = from.take_until(COPY)
        {
            sys::write_all(2, &before);
            let host = loop {
                if let Some(line) = from.take_until(b"\n") {
                    break line;
                }
                if from.fill(from_rd) != Some(true) {
                    gone(pid);
                }
            };
            if !image.send(to_wr, &host) {
                gone(pid);
            }
            continue;
        }
        if let Some(before) = from.take_until(MAGIC) {
            sys::write_all(2, &before);
            break;
        }
        if from.fill(from_rd) != Some(true) {
            gone(pid);
        }
    }
    match from.read(from_rd) {
        Some(f) if f.kind == msg::HELLO => {
            let mut d = Dec::new(&f.data);
            let (version, luish) = (d.u32(), d.string());
            if version != Some(VERSION) {
                fail(&format!(
                    "the server's luish ({}) speaks another version of the protocol than this one ({})",
                    luish.as_deref().unwrap_or("?"),
                    env!("CARGO_PKG_VERSION")
                ));
            }
        }
        _ => gone(pid),
    }
    if !write_frame(to_wr, msg::HELLO, &hello.buf) {
        gone(pid);
    }

    let (wake_rd, wake_wr) = sys::pipe().unwrap_or_else(|e| fail(&sys::strerror(e)));
    // A full pipe has woken the main thread already: the signal handler
    // must not wait.
    // SAFETY: fcntl on our own fd.
    unsafe { libc::fcntl(wake_wr, libc::F_SETFL, libc::O_NONBLOCK) };
    WAKE.store(wake_wr, Ordering::Relaxed);
    let (tx, rx) = channel();
    std::thread::spawn(move || read_server(from, from_rd, tx, wake_wr));
    let conn = Rc::new(Conn { to: to_wr, events: rx });
    if !editor::init_client(Box::new(conn.clone()), escape) {
        fail("cannot start the line editor");
    }
    Client {
        conn,
        sent: Vec::new(),
        sent_before: 0,
        pid,
        wake: wake_rd,
        saved: sys::tcgetattr(0),
        size: (cols, rows),
        escapes: Escapes::new(escape),
        escape_char: escape,
    }
    .run()
}

/// The thread that reads from the server.
fn read_server(mut from: Reader, fd: i32, tx: Sender<Event>, wake: i32) {
    loop {
        let f = from.read(fd);
        let done = f.is_none();
        match f {
            Some(f) if f.kind == msg::OUTPUT => {
                sys::write_all(1, &f.data);
                continue;
            }
            Some(f) => {
                let _ = tx.send(Some((f.kind, f.data)));
            }
            None => {
                let _ = tx.send(None);
            }
        }
        sys::write_all(wake, b"x");
        if done {
            return;
        }
    }
}

extern "C" fn on_winch(_: libc::c_int) {
    RESIZED.store(true, Ordering::Relaxed);
    let fd = WAKE.load(Ordering::Relaxed);
    // SAFETY: write is async-signal-safe.
    unsafe { libc::write(fd, b"w".as_ptr().cast(), 1) };
}

/// Exits when the connection has gone without the shell's status: with
/// the transport's status (as ssh's 255 for a connection that failed).
fn gone(pid: i32) -> ! {
    let status = match sys::waitpid(pid, 0) {
        Ok(Some((_, ws))) => ws.code(),
        _ => 255,
    };
    sys::exit(if status == 0 { 255 } else { status })
}

struct Client {
    conn: Rc<Conn>,
    /// The bytes of input sent since the last request, and the count of
    /// those sent before them, for the keys the pty didn't get.
    sent: Vec<u8>,
    sent_before: u64,
    pid: i32,
    wake: i32,
    /// The terminal's modes as they were, for the line editor and the end.
    saved: Option<libc::termios>,
    /// The window size the server has.
    size: (u16, u16),
    /// The escapes in the keys typed while a command runs, and the escape
    /// character.
    escapes: Escapes,
    escape_char: Option<u8>,
}

impl Client {
    fn run(mut self) -> ! {
        // SAFETY: a handler that only stores and writes.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_winch as *const () as usize;
            libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
        }
        self.raw();
        loop {
            let mut fds = [
                libc::pollfd {
                    fd: 0,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ready = super::poll(&mut fds, -1);
            if RESIZED.swap(false, Ordering::Relaxed) {
                self.send_size();
            }
            if !ready {
                continue;
            }
            if fds[1].revents != 0 {
                let mut buf = [0u8; 256];
                let _ = sys::read(self.wake, &mut buf, false);
                while let Ok(e) = self.conn.events.try_recv() {
                    self.event(e);
                }
                // The editor may have read the keys that were waiting.
                continue;
            }
            if fds[0].revents != 0 {
                let mut buf = [0u8; 16384];
                match sys::read(0, &mut buf, false) {
                    Ok(n @ 1..) => self.input(&buf[..n]),
                    _ => self.end(Some(0)),
                }
            }
        }
    }

    /// Keys typed while a command runs: passed on to the server's pty,
    /// apart from the escapes.
    fn input(&mut self, keys: &[u8]) {
        let mut out = Vec::with_capacity(keys.len());
        for &k in keys {
            if let Some(a) = self.escapes.key(k, &mut out) {
                self.send_input(&std::mem::take(&mut out));
                let mut echo = self.escape_char.into_iter().collect::<Vec<_>>();
                echo.extend_from_slice(escape::echo(a));
                echo.extend_from_slice(b"\r\n");
                sys::write_all(2, &echo);
                self.escape(a, false);
            }
        }
        self.send_input(&out);
    }

    fn send_input(&mut self, keys: &[u8]) {
        if keys.is_empty() {
            return;
        }
        if !self.conn.send(msg::INPUT, keys) {
            self.end(None);
        }
        self.sent.extend_from_slice(keys);
    }

    /// Does what the escape `a` asks, after its echo. While a command runs
    /// the terminal is raw, else (`editing`) as it was.
    fn escape(&self, a: Action, editing: bool) {
        match a {
            Action::Disconnect => self.disconnect(),
            Action::Suspend => {
                if !editing {
                    self.cooked();
                }
                // Only the client stops: ssh keeps the connection.
                // SAFETY: plain raise.
                unsafe { libc::raise(libc::SIGTSTP) };
                if !editing {
                    self.raw();
                }
            }
            Action::Help => {
                sys::write_all(2, &escape::help(self.escape_char.unwrap_or(b'~')));
            }
        }
    }

    /// `~.`: ends the transport (ssh, which may wait for a network that
    /// has gone) and exits with 255, as ssh's `~.`.
    fn disconnect(&self) -> ! {
        self.cooked();
        sys::write_all(2, b"luish: the connection to the server was closed\n");
        // SAFETY: plain kill.
        unsafe { libc::kill(self.pid, libc::SIGTERM) };
        for _ in 0..100 {
            if !matches!(sys::waitpid(self.pid, libc::WNOHANG), Ok(None)) {
                sys::exit(255);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // SAFETY: plain kill.
        unsafe { libc::kill(self.pid, libc::SIGKILL) };
        let _ = sys::waitpid(self.pid, 0);
        sys::exit(255)
    }

    fn event(&mut self, e: Event) {
        match e {
            Some((msg::REQUEST, data)) => {
                self.cooked();
                let held = self.escapes.line_read();
                let sent = std::mem::take(&mut self.sent);
                let before = self.sent_before;
                self.sent_before += sent.len() as u64;
                // The keys from where the relay stopped passing them on.
                let unsent = |held_at: u64| {
                    let from = held_at.saturating_sub(before).min(sent.len() as u64);
                    sent[from as usize..].to_vec()
                };
                // (The terminal is in its own modes, to exit with.)
                let reply = editor::client_line(&data, unsent, held, |a| self.escape(a, true))
                    .unwrap_or_else(|| fail("the server sent a request this client can't read"));
                // The command runs with the window's size.
                self.send_size();
                if !self.conn.send(msg::LINE, &reply.encode()) {
                    self.end(None);
                }
                self.raw();
            }
            Some((msg::EXIT, data)) => {
                let status = Dec::new(&data).u32().unwrap_or(0);
                self.end(Some(status as i32));
            }
            Some(_) => {}
            None => self.end(None),
        }
    }

    /// Sends the window's size if it has changed.
    fn send_size(&mut self) {
        let Some(size) = super::window_size(1) else { return };
        if size != self.size {
            self.size = size;
            let mut e = Enc::default();
            e.u32(size.0.into()).u32(size.1.into());
            self.conn.send(msg::RESIZE, &e.buf);
        }
    }

    /// Passes everything through, as ssh does with a pty.
    fn raw(&self) {
        if let Some(mut t) = self.saved {
            // SAFETY: a valid termios.
            unsafe { libc::cfmakeraw(&mut t) };
            sys::tcsetattr(0, &t);
        }
    }

    fn cooked(&self) {
        if let Some(t) = &self.saved {
            sys::tcsetattr(0, t);
        }
    }

    /// Ends with the shell's status, or else as the transport ended.
    fn end(&self, status: Option<i32>) -> ! {
        self.cooked();
        sys::close(self.conn.to);
        match status {
            Some(s) => {
                let _ = sys::waitpid(self.pid, 0);
                sys::exit(s)
            }
            None => {
                sys::write_all(2, b"luish: the connection to the server was lost\n");
                gone(self.pid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Ssh, String> {
        let args: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes().to_vec()).collect();
        Ssh::parse(&args)
    }

    fn words(ws: &[&str]) -> Vec<Vec<u8>> {
        ws.iter().map(|w| w.as_bytes().to_vec()).collect()
    }

    #[test]
    fn ssh_options() {
        let s = parse(&["-p", "22", "--luish-path", "~/l", "-o", "Port=3", "-oX=y", "h"]).unwrap();
        assert_eq!(
            s.cmd,
            words(&["ssh", "-T", "-p", "22", "-o", "Port=3", "-o", "X=y", "h"])
        );
        assert_eq!((s.program, s.copy), (Some(b"~/l".to_vec()), CopyMode::Auto));
        for rsh in [
            &["-e", "ssh -p 2"][..],
            &["-essh -p 2"],
            &["--rsh=ssh -p 2"],
            &["--rsh", "ssh -p 2"],
            &["--ssh-command=ssh -p 2"],
            &["--ssh-command", "ssh -p 2"],
        ] {
            let s = parse(&[rsh, &["h"]].concat()).unwrap();
            assert_eq!(s.cmd, words(&["ssh", "-p", "2", "h"]), "{rsh:?}");
        }
        assert_eq!(parse(&["-o", "ssh.no_auto_copy", "h"]).unwrap().copy, CopyMode::Never);
        assert_eq!(parse(&["-ossh.NoAutoCopy", "h"]).unwrap().copy, CopyMode::Never);
        assert_eq!(
            parse(&["-o", "ssh.no_auto_copy", "-o", "ssh.auto_copy", "h"])
                .unwrap()
                .copy,
            CopyMode::Auto
        );
        let s = parse(&["-o", "ssh.no_auto_copy", "--copy-luish", "h"]).unwrap();
        assert_eq!(s.copy, CopyMode::Always);
        assert_eq!(s.cmd, words(&["ssh", "-T", "h"]));
        assert_eq!(s.escape, Some(b'~'));
        assert_eq!(parse(&["-o", "ssh.escape_char=%", "h"]).unwrap().escape, Some(b'%'));
        assert_eq!(parse(&["-ossh.EscapeChar=none", "h"]).unwrap().escape, None);
        for bad in [
            &[][..],
            &["--copy-luish"],
            &["h", "-e"],
            &["-e", "", "h"],
            &["-e", "'ssh", "h"],
            &["-o", "ssh.copy", "h"],
            &["-o", "ssh.escape_char", "h"],
            &["-o", "ssh.escape_char=^]", "h"],
            &["-o", "ssh.escape_char= ", "h"],
            &["-o", "ssh.no_auto_copy=1", "h"],
            &["--luish-path=", "h"],
            &["--copy-luish", "--luish-path=l", "h"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rsh_words() {
        let w = |s: &str| split_words(s.as_bytes());
        assert_eq!(w(" a  b\tc "), Some(words(&["a", "b", "c"])));
        assert_eq!(
            w(r#"ssh -o 'A B' "C \"D\" \x" E\ F ''"#),
            Some(words(&["ssh", "-o", "A B", r#"C "D" \x"#, "E F", ""]))
        );
        assert_eq!(w(""), Some(vec![]));
        assert_eq!(w("a 'b"), None);
        assert_eq!(w(r#"a "b\""#), None);
    }
}
