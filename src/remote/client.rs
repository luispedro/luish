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

use super::{Dec, Enc, Reader, VERSION, msg, write_frame};
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

/// `luish --ssh [SSH-OPTION...] HOST`: `ssh -T [SSH-OPTION...] HOST luish
/// --serve`. `--luish-path=PROGRAM` (or `--luish-path PROGRAM`), anywhere
/// among the options, replaces `luish`, as rsync's `--rsync-path`: the
/// remote shell runs it, so `~/bin/luish` works. ssh's own options never
/// start with `--`.
pub fn ssh(args: &[Vec<u8>]) -> ! {
    let mut program = b"luish".to_vec();
    let mut cmd = vec![b"ssh".to_vec(), b"-T".to_vec()];
    let mut args = args.iter();
    while let Some(a) = args.next() {
        if let Some(p) = a.strip_prefix(b"--luish-path=") {
            program = p.to_vec();
        } else if a == b"--luish-path" {
            program = args
                .next()
                .unwrap_or_else(|| fail("--luish-path requires a program"))
                .clone();
        } else {
            cmd.push(a.clone());
        }
    }
    if cmd.len() == 2 {
        fail("--ssh requires a host");
    }
    if program.is_empty() {
        fail("--luish-path requires a program");
    }
    cmd.extend([program, b"--serve".to_vec()]);
    run(&cmd)
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
        if let Some(before) = from.take_until_magic() {
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
    if !editor::init_client(Box::new(conn.clone())) {
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
                    Ok(n @ 1..) => {
                        if !self.conn.send(msg::INPUT, &buf[..n]) {
                            self.end(None);
                        }
                        self.sent.extend_from_slice(&buf[..n]);
                    }
                    _ => self.end(Some(0)),
                }
            }
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Some((msg::REQUEST, data)) => {
                self.cooked();
                let sent = std::mem::take(&mut self.sent);
                let before = self.sent_before;
                self.sent_before += sent.len() as u64;
                // The keys from where the relay stopped passing them on.
                let unsent = |held_at: u64| {
                    let from = held_at.saturating_sub(before).min(sent.len() as u64);
                    sent[from as usize..].to_vec()
                };
                // (The terminal is in its own modes, to exit with.)
                let reply = editor::client_line(&data, unsent)
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
