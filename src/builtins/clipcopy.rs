//! `clipcopy`: puts its input on the system clipboard through the terminal
//! (OSC 52), which works over ssh and in tmux, without xclip or wl-copy.

use crate::shell::{ExecResult, Shell};
use crate::sys;

pub fn clipcopy(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let args = match super::options(sh, argv, b"") {
        Ok((_, args)) => args,
        Err(n) => return Ok(n),
    };
    if args.len() > 1 {
        sh.berr(&argv[0], "too many arguments");
        return Ok(2);
    }
    // First, so that nothing is read if there is no terminal.
    let tty = match sys::open(b"/dev/tty", libc::O_WRONLY | libc::O_NOCTTY, 0) {
        Ok(fd) => fd,
        Err(e) => return Ok(fail(sh, argv, "cannot open", b"/dev/tty", e)),
    };
    let (fd, name) = match args.first() {
        Some(f) if f != b"-" => match sys::open(f, libc::O_RDONLY, 0) {
            Ok(fd) => (fd, &f[..]),
            Err(e) => {
                sys::close(tty);
                return Ok(fail(sh, argv, "cannot open", f, e));
            }
        },
        _ => (0, &b"standard input"[..]),
    };
    let text = read_all(fd);
    if fd != 0 {
        sys::close(fd);
    }
    let text = match text {
        Ok(t) => t,
        Err(e) => {
            sys::close(tty);
            return Ok(fail(sh, argv, "cannot read", name, e));
        }
    };
    let ok = sys::write_all(tty, &sequence(&text));
    sys::close(tty);
    Ok(if ok { 0 } else { 1 })
}

fn fail(sh: &Shell, argv: &[Vec<u8>], what: &str, name: &[u8], e: i32) -> i32 {
    let name = String::from_utf8_lossy(name);
    sh.berr(&argv[0], format!("{what} {name}: {}", sys::strerror(e)));
    1
}

/// Everything up to the end of `fd`.
fn read_all(fd: i32) -> Result<Vec<u8>, i32> {
    let mut text = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match sys::read(fd, &mut buf, false)? {
            0 => return Ok(text),
            n => text.extend_from_slice(&buf[..n]),
        }
    }
}

/// The escape sequence that sets the clipboard to `text`.
fn sequence(text: &[u8]) -> Vec<u8> {
    let mut s = b"\x1b]52;c;".to_vec();
    base64(text, &mut s);
    s.push(0x07);
    s
}

fn base64(data: &[u8], out: &mut Vec<u8>) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]);
            } else {
                out.push(b'=');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes() {
        let b = |s: &[u8]| {
            let mut out = Vec::new();
            base64(s, &mut out);
            String::from_utf8(out).unwrap()
        };
        assert_eq!(b(b""), "");
        assert_eq!(b(b"f"), "Zg==");
        assert_eq!(b(b"fo"), "Zm8=");
        assert_eq!(b(b"foo"), "Zm9v");
        assert_eq!(b(b"foob"), "Zm9vYg==");
        assert_eq!(b(b"hi\n"), "aGkK");
        assert_eq!(b(&[0xff, 0xfe, 0x00]), "//4A");
        assert_eq!(sequence(b"hi\n"), b"\x1b]52;c;aGkK\x07");
    }
}
