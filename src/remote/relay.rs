//! The server of the SSH mode (`luish --serve`): a relay process between
//! the client (on stdin and stdout) and the shell, which runs on a pty of
//! its own as a session leader, so that commands, job control and the
//! terminal's signals work as in a local terminal.
//!
//! The relay passes what the client types to the pty while a command runs,
//! and what the pty prints to the client. The shell's messages (requests
//! for a line, answers to Tab) come on a socket. A request for a line is
//! sent only once everything the pty printed before it has been: the shell
//! writes `SYNC` to the pty before asking, and the relay takes it out.
//!
//! Keys typed while the request is on its way are not lost: asked to hold
//! (`msg::HOLD`), the relay stops writing keys to the pty until the line
//! comes back, and the shell reads those left on the pty to send them with
//! the request. The request says where in the client's input the relay
//! stopped, so that the client starts the line with the keys after it.

use std::collections::VecDeque;

use super::{Dec, Enc, Forward, Frame, MAGIC, Reader, VERSION, msg, write_frame};
use crate::interactive::remote::SYNC;
use crate::sys;

fn fail(what: &str) -> ! {
    let msg = format!("luish --serve: {what}\n");
    sys::write_all(2, msg.as_bytes());
    sys::exit(1)
}

/// Starts the server: greets the client, opens the pty and forks the shell
/// onto it. Returns in the shell, with its end of the socket to the relay,
/// after taking the client's variables for its terminal and locale
/// (`forward`); the relay never returns.
pub fn start() -> i32 {
    let mut hello = Enc::default();
    hello.u32(VERSION).str(env!("CARGO_PKG_VERSION"));
    if !sys::write_all(1, MAGIC) || !write_frame(1, msg::HELLO, &hello.buf) {
        sys::exit(1);
    }
    let mut from_client = Reader::default();
    let Some(f) = from_client.read(0) else { sys::exit(1) };
    let mut d = Dec::new(&f.data);
    if f.kind != msg::HELLO || d.u32() != Some(VERSION) {
        // The client reports it, having our version.
        sys::exit(1);
    }
    let (Some(cols), Some(rows), Some(vars)) = (d.u32(), d.u32(), d.list(|d| Some((d.bytes()?, d.bytes()?)))) else {
        fail("the client sent no greeting");
    };
    for (name, value) in vars {
        let Some(how) = super::forward(&name) else { continue };
        let (name, value) = (sys::cstr(&name), sys::cstr(&value));
        // SAFETY: single-threaded, before anything reads the environment.
        unsafe { libc::setenv(name.as_ptr(), value.as_ptr(), (how == Forward::Replace).into()) };
    }
    let ws = libc::winsize {
        ws_row: rows as u16,
        ws_col: cols as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let (mut master, mut slave) = (0, 0);
    // SAFETY: valid out-pointers and winsize.
    if unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &ws) } < 0 {
        fail(&format!("cannot open a pty: {}", sys::strerror(sys::errno())));
    }
    let mut sv = [0; 2];
    // SAFETY: valid output array.
    if unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_CLOEXEC,
            0,
            sv.as_mut_ptr(),
        )
    } < 0
    {
        fail(&format!("socketpair: {}", sys::strerror(sys::errno())));
    }
    let pid = sys::fork().unwrap_or_else(|e| fail(&format!("fork: {}", sys::strerror(e))));
    if pid == 0 {
        sys::close(master);
        sys::close(sv[0]);
        // SAFETY: plain calls on our own fds; the pty becomes the
        // controlling terminal of the new session.
        unsafe {
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY, 0);
        }
        for fd in 0..3 {
            let _ = sys::dup2(slave, fd);
        }
        if slave > 2 {
            sys::close(slave);
        }
        return sv[1];
    }
    sys::close(slave);
    sys::close(sv[1]);
    // The pty's output is read as it comes, and the shell's messages as
    // they come, so neither blocks the relay.
    // SAFETY: fcntl on our own fd.
    unsafe {
        libc::fcntl(
            master,
            libc::F_SETFL,
            libc::fcntl(master, libc::F_GETFL) | libc::O_NONBLOCK,
        )
    };
    Relay {
        master,
        ctl: sv[0],
        pid,
        from_client,
        from_shell: Reader::default(),
        held: VecDeque::new(),
        syncs: 0,
        kept: Vec::new(),
        to_pty: Vec::new(),
        received: 0,
        held_at: None,
    }
    .run()
}

struct Relay {
    master: i32,
    /// The socket to the shell.
    ctl: i32,
    pid: i32,
    from_client: Reader,
    from_shell: Reader,
    /// Requests for a line waiting for their `SYNC`.
    held: VecDeque<Frame>,
    /// `SYNC`s seen that no request has waited for yet.
    syncs: usize,
    /// The end of the pty's output, kept back while it may be the start of
    /// a `SYNC`.
    kept: Vec<u8>,
    /// Keys for the pty that it hasn't taken yet.
    to_pty: Vec<u8>,
    /// The bytes of input the client has sent.
    received: u64,
    /// While the shell waits for a line: how many of them went to the pty.
    held_at: Option<u64>,
}

impl Relay {
    fn run(mut self) -> ! {
        let mut pty_open = true;
        // Once the socket closes, the shell has exited, or `exec`ed another
        // program, which then has the pty to itself until it exits.
        let mut ctl_open = true;
        loop {
            while let Some(f) = self
                .from_client
                .next()
                .unwrap_or_else(|()| fail("bad message from the client"))
            {
                self.client_frame(f);
            }
            let mut fds = [
                libc::pollfd {
                    fd: 0,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if ctl_open { self.ctl } else { -1 },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if pty_open { self.master } else { -1 },
                    events: libc::POLLIN | if self.to_pty.is_empty() { 0 } else { libc::POLLOUT },
                    revents: 0,
                },
            ];
            // What may be the start of a `SYNC` waits only briefly; without
            // the socket, the process is checked for now and then.
            let timeout = match (self.kept.is_empty(), ctl_open) {
                (false, _) => 50,
                (true, false) => 100,
                (true, true) => -1,
            };
            let ready = super::poll(&mut fds, timeout);
            if !ctl_open && let Ok(Some((_, ws))) = sys::waitpid(self.pid, libc::WNOHANG) {
                self.finish(ws.code());
            }
            if !ready {
                self.flush();
                continue;
            }
            if fds[2].revents & libc::POLLOUT != 0 {
                self.write_pty();
            }
            if fds[2].revents & !libc::POLLOUT != 0 && !self.read_pty() {
                pty_open = false;
            }
            if fds[1].revents != 0 {
                match self.from_shell.fill(self.ctl) {
                    Some(true) => {
                        while let Ok(Some(f)) = self.from_shell.next() {
                            self.shell_frame(f);
                        }
                    }
                    _ => ctl_open = false,
                }
            }
            if fds[0].revents != 0 && self.from_client.fill(0) != Some(true) {
                // The client has gone: closing the pty hangs up the shell.
                sys::close(self.master);
                sys::exit(0);
            }
        }
    }

    fn client_frame(&mut self, f: Frame) {
        match f.kind {
            msg::INPUT => {
                self.received += f.data.len() as u64;
                if self.held_at.is_none() {
                    self.to_pty.extend_from_slice(&f.data);
                    self.write_pty();
                }
            }
            msg::RESIZE => {
                let mut d = Dec::new(&f.data);
                if let (Some(c), Some(r)) = (d.u32(), d.u32()) {
                    super::set_window_size(self.master, c as u16, r as u16);
                }
            }
            kind => {
                if kind == msg::LINE {
                    self.held_at = None;
                }
                write_frame(self.ctl, f.kind, &f.data);
            }
        }
    }

    fn shell_frame(&mut self, f: Frame) {
        match f.kind {
            msg::HOLD => {
                self.held_at = Some(self.received - self.to_pty.len() as u64);
                self.to_pty.clear();
                write_frame(self.ctl, msg::HELD, &[]);
            }
            msg::REQUEST => {
                self.held.push_back(f);
                self.release();
            }
            _ => self.to_client(f.kind, &f.data),
        }
    }

    fn to_client(&self, kind: u8, data: &[u8]) {
        if !write_frame(1, kind, data) {
            sys::close(self.master);
            sys::exit(0);
        }
    }

    /// Sends the requests whose `SYNC` has come.
    fn release(&mut self) {
        while self.syncs > 0 {
            let Some(f) = self.held.pop_front() else { break };
            self.syncs -= 1;
            let mut e = Enc::default();
            e.u64(self.held_at.unwrap_or(self.received));
            e.buf.extend_from_slice(&f.data);
            self.to_client(f.kind, &e.buf);
        }
    }

    /// Writes as many of the keys typed as the pty takes now.
    fn write_pty(&mut self) {
        while !self.to_pty.is_empty() {
            // SAFETY: a valid buffer of the length given.
            let n = unsafe { libc::write(self.master, self.to_pty.as_ptr().cast(), self.to_pty.len()) };
            match n {
                1.. => drop(self.to_pty.drain(..n as usize)),
                _ if sys::errno() == libc::EINTR => {}
                _ if sys::errno() == libc::EAGAIN => return,
                // The pty has gone: the keys with it.
                _ => self.to_pty.clear(),
            }
        }
    }

    /// Passes on what the pty printed. False once the pty is closed (no
    /// process has it open any more).
    fn read_pty(&mut self) -> bool {
        let mut buf = [0u8; 16384];
        match sys::read(self.master, &mut buf, false) {
            Ok(0) => false,
            Ok(n) => {
                self.output(&buf[..n]);
                true
            }
            Err(libc::EAGAIN) => true,
            Err(_) => false,
        }
    }

    /// Passes on the pty's output, without the `SYNC`s, then the requests
    /// they let through.
    fn output(&mut self, data: &[u8]) {
        self.kept.extend_from_slice(data);
        let mut out = Vec::with_capacity(self.kept.len());
        let mut rest = &self.kept[..];
        while let Some(i) = rest.windows(SYNC.len()).position(|w| w == SYNC) {
            out.extend_from_slice(&rest[..i]);
            rest = &rest[i + SYNC.len()..];
            self.syncs += 1;
        }
        // The longest end that may be the start of a `SYNC` is kept back.
        let keep = (1..SYNC.len().min(rest.len() + 1))
            .rev()
            .find(|&n| SYNC.starts_with(&rest[rest.len() - n..]))
            .unwrap_or(0);
        out.extend_from_slice(&rest[..rest.len() - keep]);
        self.kept = rest[rest.len() - keep..].to_vec();
        if !out.is_empty() {
            self.to_client(msg::OUTPUT, &out);
        }
        self.release();
    }

    /// Sends what was kept back, when nothing more has come.
    fn flush(&mut self) {
        let kept = std::mem::take(&mut self.kept);
        if !kept.is_empty() {
            self.to_client(msg::OUTPUT, &kept);
        }
    }

    /// The shell has exited, with `status`: sends the rest of the output,
    /// until no process has the pty open or nothing more comes for a while,
    /// then the status.
    fn finish(&mut self, status: i32) -> ! {
        loop {
            let mut fds = [libc::pollfd {
                fd: self.master,
                events: libc::POLLIN,
                revents: 0,
            }];
            if !super::poll(&mut fds, 200) || !self.read_pty() {
                break;
            }
        }
        self.flush();
        let mut e = Enc::default();
        e.u32(status as u32);
        self.to_client(msg::EXIT, &e.buf);
        sys::exit(0)
    }
}
