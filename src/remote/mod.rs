//! The SSH mode (Stage 3): the line editor runs on the client (`luish
//! --remote CMD...`), while the shell runs on the server (`luish --serve`,
//! at the other end of CMD, usually ssh). See `DEVELOPING.md`, SSH mode.
//!
//! This module is the protocol: frames of a type byte and a length, over
//! the transport's pipes, and on the server between the relay and the shell
//! (`relay.rs`). The editor's side (what a line request holds) is in
//! `interactive/remote.rs`; the client in `client.rs`.

pub mod chaos;
pub mod client;
pub mod escape;
pub mod relay;

/// Bumped whenever a message changes: both ends must speak the same.
pub const VERSION: u32 = 3;

/// What the server writes before its hello, so that the client can skip
/// what a remote startup file printed before luish started.
pub const MAGIC: &[u8] = b"\0luish-serve\0";

/// What the script that `--ssh` runs on the server writes when it has no
/// copy of this luish, followed by `uname -sm` and a newline
/// (`client::Image`).
pub const COPY: &[u8] = b"\0luish-copy\0";

/// The message types. The relay handles those of the transport (hello,
/// input, resize, output, exit) and passes the others between the client
/// and the shell.
pub mod msg {
    /// Client to server: the protocol version, the window size, and the
    /// variables of its environment that `forward` names. Server to client:
    /// the protocol version.
    pub const HELLO: u8 = b'H';
    /// Client to server: keys for the pty, while a command runs.
    pub const INPUT: u8 = b'I';
    /// Client to server: the terminal's new size.
    pub const RESIZE: u8 = b'W';
    /// Server to client: what the pty printed.
    pub const OUTPUT: u8 = b'O';
    /// Server to client: the shell exited, with this status.
    pub const EXIT: u8 = b'X';
    /// Shell to relay, before a request: stop writing keys to the pty.
    /// Answered with `HELD`, after which the keys left on the pty are the
    /// shell's to read.
    pub const HOLD: u8 = b'h';
    pub const HELD: u8 = b'd';
    /// Shell to client: read a line (`interactive::remote::Request`). The
    /// relay puts first how many bytes of input it had passed to the pty
    /// when it stopped (the ones after are the client's to use).
    pub const REQUEST: u8 = b'R';
    /// Client to shell: the line read.
    pub const LINE: u8 = b'L';
    /// Client to shell: Tab was pressed, with the line before and after
    /// the cursor. Answered with `TAB_REPLY`.
    pub const TAB: u8 = b'T';
    pub const TAB_REPLY: u8 = b't';
    /// Client to shell: a question about a file, for the highlighter.
    /// Answered with `LOOKUP_REPLY`.
    pub const LOOKUP: u8 = b'F';
    pub const LOOKUP_REPLY: u8 = b'f';
}

/// How the server takes a variable of the client's environment.
#[derive(Debug, PartialEq)]
pub enum Forward {
    /// It describes the client's terminal, so it replaces the server's (as
    /// ssh does with `TERM` when it opens a pty).
    Replace,
    /// The locale, used where the server has none (ssh often forwards it
    /// already, through `SendEnv`).
    IfUnset,
}

/// Whether the client sends the variable `name` to the server, and how the
/// server takes it. The others stay the server's.
pub fn forward(name: &[u8]) -> Option<Forward> {
    match name {
        b"TERM" | b"COLORTERM" | b"TERM_PROGRAM" | b"TERM_PROGRAM_VERSION" => Some(Forward::Replace),
        b"LANG" | b"LANGUAGE" => Some(Forward::IfUnset),
        _ if name.starts_with(b"LC_") => Some(Forward::IfUnset),
        _ => None,
    }
}

/// The most a frame may hold, so that garbage isn't taken for a length.
const MAX_FRAME: usize = 1 << 28;

/// A frame: its type and its payload.
pub struct Frame {
    pub kind: u8,
    pub data: Vec<u8>,
}

/// A frame's bytes.
pub fn frame(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + data.len());
    buf.push(kind);
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
    buf
}

/// Writes a frame. False if the other end has gone.
pub fn write_frame(fd: i32, kind: u8, data: &[u8]) -> bool {
    crate::sys::write_all(fd, &frame(kind, data))
}

/// Collects bytes read from a stream and cuts them into frames.
#[derive(Default)]
pub struct Reader {
    buf: Vec<u8>,
}

impl Reader {
    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// The next whole frame, if there is one. An impossible length is an
    /// error.
    pub fn next(&mut self) -> Result<Option<Frame>, ()> {
        if self.buf.len() < 5 {
            return Ok(None);
        }
        let len = u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
        if len > MAX_FRAME {
            return Err(());
        }
        if self.buf.len() < 5 + len {
            return Ok(None);
        }
        let kind = self.buf[0];
        let data = self.buf[5..5 + len].to_vec();
        self.buf.drain(..5 + len);
        Ok(Some(Frame { kind, data }))
    }

    /// Reads from `fd` until a whole frame has come. None at end of file
    /// or on an error.
    pub fn read(&mut self, fd: i32) -> Option<Frame> {
        loop {
            if let Some(f) = self.next().ok()? {
                return Some(f);
            }
            if !self.fill(fd)? {
                return None;
            }
        }
    }

    /// Reads once from `fd` into the buffer: Some(false) at end of file,
    /// None on an error.
    pub fn fill(&mut self, fd: i32) -> Option<bool> {
        let mut chunk = [0u8; 16384];
        match crate::sys::read(fd, &mut chunk, false) {
            Ok(0) => Some(false),
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                Some(true)
            }
            Err(_) => None,
        }
    }

    /// Takes the bytes before `mark` (such as `MAGIC`), once it has come
    /// (and drops it). None until then.
    pub fn take_until(&mut self, mark: &[u8]) -> Option<Vec<u8>> {
        let i = self.buf.windows(mark.len()).position(|w| w == mark)?;
        let before = self.buf[..i].to_vec();
        self.buf.drain(..i + mark.len());
        Some(before)
    }
}

/// Builds a message.
#[derive(Default)]
pub struct Enc {
    pub buf: Vec<u8>,
}

impl Enc {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_be_bytes());
        self
    }

    pub fn usize(&mut self, v: usize) -> &mut Self {
        self.u64(v as u64)
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.buf.extend_from_slice(v);
        self
    }

    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }

    pub fn opt_bytes(&mut self, v: Option<&[u8]>) -> &mut Self {
        match v {
            Some(v) => self.u8(1).bytes(v),
            None => self.u8(0),
        }
    }

    pub fn opt_str(&mut self, v: Option<&str>) -> &mut Self {
        self.opt_bytes(v.map(str::as_bytes))
    }

    pub fn list<T>(&mut self, items: &[T], mut f: impl FnMut(&mut Self, &T)) -> &mut Self {
        self.u32(items.len() as u32);
        for i in items {
            f(self, i);
        }
        self
    }
}

/// Reads a message. Every getter gives None past the end, so a short
/// message is an error rather than a panic.
pub struct Dec<'a> {
    data: &'a [u8],
}

impl<'a> Dec<'a> {
    pub fn new(data: &'a [u8]) -> Dec<'a> {
        Dec { data }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.data.len() < n {
            return None;
        }
        let (a, b) = self.data.split_at(n);
        self.data = b;
        Some(a)
    }

    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub fn bool(&mut self) -> Option<bool> {
        Some(self.u8()? != 0)
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn usize(&mut self) -> Option<usize> {
        usize::try_from(self.u64()?).ok()
    }

    pub fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u32()? as usize;
        Some(self.take(n)?.to_vec())
    }

    pub fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?).ok()
    }

    pub fn opt_bytes(&mut self) -> Option<Option<Vec<u8>>> {
        match self.u8()? {
            0 => Some(None),
            _ => Some(Some(self.bytes()?)),
        }
    }

    pub fn opt_string(&mut self) -> Option<Option<String>> {
        match self.opt_bytes()? {
            None => Some(None),
            Some(b) => Some(Some(String::from_utf8(b).ok()?)),
        }
    }

    pub fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let n = self.u32()? as usize;
        // Not trusting `n` for the capacity.
        let mut out = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            out.push(f(self)?);
        }
        Some(out)
    }
}

/// The line read by the client.
pub enum LineReply {
    Text(Vec<u8>),
    Interrupted,
    Eof,
}

impl LineReply {
    pub fn encode(&self) -> Vec<u8> {
        let mut e = Enc::default();
        match self {
            LineReply::Text(t) => e.u8(0).bytes(t),
            LineReply::Interrupted => e.u8(1),
            LineReply::Eof => e.u8(2),
        };
        e.buf
    }

    pub fn decode(data: &[u8]) -> Option<LineReply> {
        let mut d = Dec::new(data);
        Some(match d.u8()? {
            0 => LineReply::Text(d.bytes()?),
            1 => LineReply::Interrupted,
            _ => LineReply::Eof,
        })
    }
}

/// `poll(2)`, waiting at most `timeout` milliseconds (or for ever if -1).
/// False if nothing is ready, or a signal came.
pub fn poll(fds: &mut [libc::pollfd], timeout: i32) -> bool {
    // SAFETY: a valid array of `pollfd`s, of the length given.
    unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) > 0 }
}

/// The window size of the terminal at `fd`, as columns and rows.
pub fn window_size(fd: i32) -> Option<(u16, u16)> {
    let (c, r) = crate::sys::window_size(fd)?;
    Some((c as u16, r as u16))
}

/// Sets the window size of the terminal at `fd`.
pub fn set_window_size(fd: i32, cols: u16, rows: u16) {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: TIOCSWINSZ with a valid winsize.
    unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        let mut r = Reader::default();
        let mut wire = Vec::new();
        for (k, d) in [(b'A', &b"hello"[..]), (b'B', b""), (b'C', b"x")] {
            wire.push(k);
            wire.extend_from_slice(&(d.len() as u32).to_be_bytes());
            wire.extend_from_slice(d);
        }
        // A byte at a time, as a slow transport would give them.
        let mut got = Vec::new();
        for b in wire {
            r.feed(&[b]);
            while let Some(f) = r.next().unwrap() {
                got.push((f.kind, f.data));
            }
        }
        assert_eq!(got, [(b'A', b"hello".to_vec()), (b'B', vec![]), (b'C', b"x".to_vec())]);
    }

    #[test]
    fn forwarded() {
        assert_eq!(forward(b"COLORTERM"), Some(Forward::Replace));
        assert_eq!(forward(b"LC_CTYPE"), Some(Forward::IfUnset));
        assert_eq!(forward(b"LANG"), Some(Forward::IfUnset));
        assert_eq!(forward(b"PATH"), None);
        assert_eq!(forward(b"TERMINFO"), None);
    }

    #[test]
    fn magic() {
        let mut r = Reader::default();
        r.feed(b"motd\n\0luish-se");
        assert!(r.take_until(MAGIC).is_none());
        r.feed(b"rve\0H");
        assert_eq!(r.take_until(MAGIC).unwrap(), b"motd\n");
        r.feed(&[0, 0, 0, 0]);
        assert_eq!(r.next().unwrap().unwrap().kind, b'H');
    }

    #[test]
    fn encoding() {
        let mut e = Enc::default();
        e.u8(7)
            .bool(true)
            .u32(9)
            .usize(1 << 40)
            .bytes(b"ab")
            .opt_str(None)
            .opt_str(Some("c"));
        e.list(&[1u32, 2], |e, v| {
            e.u32(*v);
        });
        let mut d = Dec::new(&e.buf);
        assert_eq!(d.u8(), Some(7));
        assert_eq!(d.bool(), Some(true));
        assert_eq!(d.u32(), Some(9));
        assert_eq!(d.usize(), Some(1 << 40));
        assert_eq!(d.bytes().as_deref(), Some(&b"ab"[..]));
        assert_eq!(d.opt_string(), Some(None));
        assert_eq!(d.opt_string(), Some(Some("c".into())));
        assert_eq!(d.list(|d| d.u32()), Some(vec![1, 2]));
        // Past the end.
        assert_eq!(d.u8(), None);
        assert_eq!(Dec::new(&[0, 0, 0, 5, b'a']).bytes(), None);
    }
}
