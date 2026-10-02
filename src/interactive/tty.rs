//! The terminal outside the line editor: raw mode, and asking the terminal
//! for its background colour (DEVELOPING.md, Highlighting).

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

/// OSC 11 asks for the background colour, then DA1 for the terminal's
/// attributes, which every terminal answers, after the OSC 11 answer if
/// it gives one.
const QUERY: &[u8] = b"\x1b]11;?\x1b\\\x1b[c";

/// How long to wait for the answers, for a terminal (or a line) that
/// answers nothing.
const TIMEOUT: Duration = Duration::from_millis(500);

/// What the terminal answered.
#[derive(Debug, Default, PartialEq)]
pub struct Answer {
    /// The background, if the terminal told its colour.
    pub background: Option<Background>,
    /// The text typed before the answers came, up to the first control
    /// character (so Enter runs nothing).
    pub typed: Vec<u8>,
}

/// Asks the terminal (stdin and stderr) for its background colour, and
/// waits for the answers. None if they didn't come.
pub fn ask_background() -> Option<Answer> {
    let raw = Raw::new()?;
    if !sys::write_all(2, QUERY) {
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
/// OSC 11 one (`ESC ] 11 ; rgb:R/G/B`, ended by BEL or `ESC \`) comes
/// before it if at all. Other bytes were typed.
fn parse(input: &[u8]) -> Option<Answer> {
    let mut a = Answer::default();
    let mut typed = Vec::new();
    let mut i = 0;
    let mut done = false;
    while i < input.len() {
        let rest = &input[i..];
        if let Some(osc) = rest.strip_prefix(b"\x1b]11;") {
            let len = osc.iter().position(|&c| c == 0x07 || c == 0x1b)?;
            a.background = color_background(&osc[..len]);
            i += 5 + len + if osc[len] == 0x1b { 2 } else { 1 };
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
        let p = |s: &str| parse(s.as_bytes());
        let answer = |background, typed: &str| {
            Some(Answer {
                background,
                typed: typed.into(),
            })
        };
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
}
