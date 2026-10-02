//! The terminal outside the line editor: raw mode, and asking the terminal
//! for its colours: its background (DEVELOPING.md, Highlighting), and those
//! that a colour scheme sets (`termcolors.rs`).

use std::time::{Duration, Instant};

use crate::style::Background;
use crate::sys;

/// Whether a byte can be read from fd 0 within `ms` milliseconds.
pub fn readable(ms: i32) -> bool {
    let mut pfd = libc::pollfd {
        fd: 0,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll on one valid pollfd.
    unsafe { libc::poll(&mut pfd, 1, ms) > 0 }
}

/// Reads a byte from fd 0 (in raw mode), retrying after a signal.
pub fn read_byte() -> Option<u8> {
    let mut buf = [0u8; 1];
    loop {
        match sys::read(0, &mut buf, true) {
            Ok(1) => return Some(buf[0]),
            Err(libc::EINTR) => {}
            _ => return None,
        }
    }
}

/// The terminal in raw mode (no echo, no line buffering, no signals from
/// keys) while it lives; its modes are restored when it is dropped.
pub struct Raw(libc::termios);

impl Raw {
    pub fn new() -> Option<Raw> {
        let old = sys::tcgetattr(0)?;
        let mut raw = old;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
        raw.c_iflag &= !(libc::ICRNL | libc::IXON);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        sys::tcsetattr(0, &raw);
        Some(Raw(old))
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        sys::tcsetattr(0, &self.0);
    }
}

/// The question for the colours `keys` (OSC codes: `11` for the
/// background, `4;N` for colour N of the palette), then DA1 for the
/// terminal's attributes, which every terminal answers, after the colours
/// it gives.
fn query(keys: &[String]) -> Vec<u8> {
    let mut q = Vec::new();
    for k in keys {
        q.extend_from_slice(format!("\x1b]{k};?\x1b\\").as_bytes());
    }
    q.extend_from_slice(b"\x1b[c");
    q
}

/// How long to wait for the answers, for a terminal (or a line) that
/// answers nothing.
const TIMEOUT: Duration = Duration::from_millis(500);

/// What the terminal answered.
#[derive(Debug, Default, PartialEq)]
pub struct Answer {
    /// The colours it told, by their OSC code (`11`, `4;1`), as it gave
    /// them (`rgb:ffff/ffff/ffff`).
    pub colors: Vec<(String, Vec<u8>)>,
    /// The text typed before the answers came, up to the first control
    /// character (so Enter runs nothing).
    pub typed: Vec<u8>,
}

impl Answer {
    /// The colour `key`, if the terminal told it.
    pub fn color(&self, key: &str) -> Option<&[u8]> {
        self.colors.iter().find(|c| c.0 == key).map(|c| c.1.as_slice())
    }

    /// The background, if the terminal told its colour.
    pub fn background(&self) -> Option<Background> {
        color_background(self.color("11")?)
    }
}

/// Asks the terminal (stdin and stderr) for its background colour, and
/// waits for the answers. None if they didn't come.
pub fn ask_background() -> Option<Answer> {
    ask(&["11".to_owned()])
}

/// Asks the terminal (stdin and stderr) for the colours `keys` (OSC codes),
/// and waits for the answers. None if they didn't come.
pub fn ask(keys: &[String]) -> Option<Answer> {
    let raw = Raw::new()?;
    if !sys::write_all(2, &query(keys)) {
        return None;
    }
    let end = Instant::now() + TIMEOUT;
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let answer = loop {
        if let Some(a) = parse(&got) {
            break Some(a);
        }
        let left = end.saturating_duration_since(Instant::now()).as_millis() as i32;
        if left == 0 || !readable(left) {
            break None;
        }
        match sys::read(0, &mut buf, true) {
            Ok(n) if n > 0 => got.extend_from_slice(&buf[..n]),
            Err(libc::EINTR) => {}
            _ => break None,
        }
    };
    drop(raw);
    answer
}

/// The answers in `input`, if it has the DA1 one (`ESC [ ? ... c`); the
/// colours' (`ESC ] 11 ; rgb:R/G/B` and `ESC ] 4 ; N ; rgb:R/G/B`, ended
/// by BEL or `ESC \`) come before it if at all. Other bytes were typed.
fn parse(input: &[u8]) -> Option<Answer> {
    let mut a = Answer::default();
    let mut typed = Vec::new();
    let mut i = 0;
    let mut done = false;
    while i < input.len() {
        let rest = &input[i..];
        if let Some(osc) = rest.strip_prefix(b"\x1b]") {
            let len = osc.iter().position(|&c| c == 0x07 || c == 0x1b)?;
            if let Some(c) = osc_color(&osc[..len]) {
                a.colors.push(c);
            }
            i += 2 + len + if osc[len] == 0x1b { 2 } else { 1 };
            continue;
        }
        if let Some(csi) = rest.strip_prefix(b"\x1b[?") {
            let len = csi.iter().position(|c| !matches!(c, b'0'..=b'9' | b';'))?;
            if csi[len] == b'c' {
                done = true;
                i += 3 + len + 1;
                continue;
            }
        }
        typed.push(input[i]);
        i += 1;
    }
    let printable = typed.iter().position(|&c| c < 0x20 || c == 0x7f).unwrap_or(typed.len());
    typed.truncate(printable);
    a.typed = typed;
    done.then_some(a)
}

/// The colour that an OSC answer gives (`11;rgb:...` or `4;1;rgb:...`),
/// by its key (`11`, `4;1`).
fn osc_color(osc: &[u8]) -> Option<(String, Vec<u8>)> {
    let text = std::str::from_utf8(osc).ok()?;
    let (code, rest) = text.split_once(';')?;
    let (key, value) = match code {
        "4" => {
            let (n, value) = rest.split_once(';')?;
            n.parse::<u8>().ok()?;
            (format!("4;{n}"), value)
        }
        _ if !code.is_empty() && code.bytes().all(|c| c.is_ascii_digit()) => (code.to_owned(), rest),
        _ => return None,
    };
    Some((key, value.as_bytes().to_vec()))
}

/// Whether a colour of the form `rgb:R/G/B` (or `rgba:R/G/B/A`), with 1
/// to 4 hex digits per channel, is dark or light: dark if its perceived
/// lightness (CIE L*) is below 50.
fn color_background(text: &[u8]) -> Option<Background> {
    let text = std::str::from_utf8(text).ok()?;
    let channels = text.strip_prefix("rgb:").or_else(|| text.strip_prefix("rgba:"))?;
    let mut y = 0.0;
    let mut parts = channels.split('/');
    for weight in [0.2126, 0.7152, 0.0722] {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 4 {
            return None;
        }
        let max = (1u32 << (4 * p.len())) - 1;
        let c = u32::from_str_radix(p, 16).ok()? as f64 / max as f64;
        // From sRGB to linear.
        let lin = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
        y += weight * lin;
    }
    // The relative luminance at L* = 50: ((50 + 16) / 116)^3.
    Some(if y < 0.184_186_5 {
        Background::Dark
    } else {
        Background::Light
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        let bg = |s: &str| color_background(s.as_bytes());
        assert_eq!(bg("rgb:0000/0000/0000"), Some(Background::Dark));
        assert_eq!(bg("rgb:ffff/ffff/ffff"), Some(Background::Light));
        // Solarized's dark and light backgrounds.
        assert_eq!(bg("rgb:0000/2b2b/3636"), Some(Background::Dark));
        assert_eq!(bg("rgb:fdfd/f6f6/e3e3"), Some(Background::Light));
        // Mid-grey is light to the eye, though its luminance is 0.22.
        assert_eq!(bg("rgb:80/80/80"), Some(Background::Light));
        assert_eq!(bg("rgb:6/6/6"), Some(Background::Dark));
        assert_eq!(bg("rgba:ffff/ffff/ffff/0000"), Some(Background::Light));
        assert_eq!(bg("rgb:fffff/0/0"), None);
        assert_eq!(bg("rgb:ff/ff"), None);
        assert_eq!(bg("#ffffff"), None);
    }

    #[test]
    fn answers() {
        let light = Some(Background::Light);
        let p = |s: &str| parse(s.as_bytes()).map(|a| (a.background(), a.typed));
        let answer = |background, typed: &str| Some((background, typed.as_bytes().to_vec()));
        assert_eq!(p("\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[?62;22c"), answer(light, ""));
        assert_eq!(p("\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?6c"), answer(light, ""));
        // Only DA1: the terminal doesn't tell.
        assert_eq!(p("\x1b[?1;2c"), answer(None, ""));
        // Not yet the DA1 answer.
        assert_eq!(p("\x1b]11;rgb:ffff/ffff/ffff\x1b\\"), None);
        assert_eq!(p("\x1b]11;rgb:ff"), None);
        assert_eq!(p("\x1b[?62;"), None);
        // Typed text, kept up to the first control character, around the
        // answers and an arrow key.
        assert_eq!(
            p("ls -l\x1b]11;rgb:ffff/ffff/ffff\x1b\\ é\x1b[?6c"),
            answer(light, "ls -l é")
        );
        assert_eq!(p("ls\r\x1b[?6cx"), answer(None, "ls"));
        assert_eq!(p("a\x1b[Ab\x1b[?6c"), answer(None, "a"));
    }

    #[test]
    fn color_answers() {
        let a =
            parse(b"\x1b]10;rgb:ebeb/dbdb/b2b2\x1b\\\x1b]4;1;rgb:cccc/2424/1d1d\x07\x1b]4;12;rgb:0/0/ff\x1b\\\x1b[?6c")
                .unwrap();
        assert_eq!(a.color("10"), Some(&b"rgb:ebeb/dbdb/b2b2"[..]));
        assert_eq!(a.color("4;1"), Some(&b"rgb:cccc/2424/1d1d"[..]));
        assert_eq!(a.color("4;12"), Some(&b"rgb:0/0/ff"[..]));
        assert_eq!(a.color("11"), None);
        assert_eq!(a.background(), None);
        // Other OSC sequences, or malformed ones, aren't colours.
        let a = parse(b"\x1b]4;x;rgb:0/0/0\x07\x1b]l;title\x07\x1b]11;rgb:0/0/0\x07\x1b[?6c").unwrap();
        assert_eq!(a.colors, vec![("11".to_owned(), b"rgb:0/0/0".to_vec())]);
        assert_eq!(a.typed, b"");
        assert_eq!(
            query(&["11".into(), "4;3".into()]),
            b"\x1b]11;?\x1b\\\x1b]4;3;?\x1b\\\x1b[c"
        );
    }
}
