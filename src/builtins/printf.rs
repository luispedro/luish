//! `printf`.

use super::echo::echo_escapes;
use crate::shell::{ExecResult, Shell};
use crate::sys;

struct Args<'a> {
    args: &'a [Vec<u8>],
    next: usize,
    status: i32,
}

impl Args<'_> {
    fn next(&mut self) -> Option<&[u8]> {
        let a = self.args.get(self.next)?;
        self.next += 1;
        Some(a)
    }

    fn report(&mut self, sh: &Shell, arg: &[u8], msg: &str) {
        sh.berr(b"printf", format!("{}: {msg}", String::from_utf8_lossy(arg)));
        self.status = 1;
    }

    fn int(&mut self, sh: &Shell) -> i64 {
        self.number(sh, true)
    }

    /// dash's `getuintmax`: an argument converted with `strtoll` (signed)
    /// or `strtoull` (for the unsigned conversions, where `-1` wraps).
    /// The result is the bits of either.
    fn number(&mut self, sh: &Shell, signed: bool) -> i64 {
        let Some(a) = self.next() else { return 0 };
        let a = a.to_vec();
        if let Some(&q) = a.first()
            && (q == b'\'' || q == b'"')
        {
            return a.get(1).copied().unwrap_or(0) as i64;
        }
        let c = sys::cstr(&a);
        let mut end: *mut libc::c_char = std::ptr::null_mut();
        // SAFETY: valid C string; strtoll/strtoull set `end` inside it.
        let (v, consumed, err) = unsafe {
            *libc::__errno_location() = 0;
            let v = if signed {
                libc::strtoll(c.as_ptr(), &mut end, 0)
            } else {
                libc::strtoull(c.as_ptr(), &mut end, 0) as i64
            };
            (v, end.offset_from(c.as_ptr()) as usize, *libc::__errno_location())
        };
        if a.is_empty() {
            return 0;
        }
        if consumed == 0 {
            self.report(sh, &a, "expected numeric value");
        } else if consumed < a.len() {
            self.report(sh, &a, "not completely converted");
        } else if err == libc::ERANGE {
            self.report(sh, &a, &sys::strerror(libc::ERANGE));
        }
        v
    }

    fn float(&mut self, sh: &Shell) -> f64 {
        let Some(a) = self.next() else { return 0.0 };
        let a = a.to_vec();
        if let Some(&q) = a.first()
            && (q == b'\'' || q == b'"')
        {
            return a.get(1).copied().unwrap_or(0) as f64;
        }
        let c = sys::cstr(&a);
        let mut end: *mut libc::c_char = std::ptr::null_mut();
        // SAFETY: valid C string.
        let (v, consumed) = unsafe {
            let v = libc::strtod(c.as_ptr(), &mut end);
            (v, end.offset_from(c.as_ptr()) as usize)
        };
        if a.is_empty() {
            return 0.0;
        }
        if consumed == 0 {
            self.report(sh, &a, "expected numeric value");
        } else if consumed < a.len() {
            self.report(sh, &a, "not completely converted");
        }
        v
    }
}

fn pad(out: &mut Vec<u8>, s: &[u8], width: Option<usize>, prec: Option<usize>, left: bool) {
    let s = match prec {
        Some(p) if p < s.len() => &s[..p],
        _ => s,
    };
    let w = width.unwrap_or(0);
    let fill = w.saturating_sub(s.len());
    if !left {
        out.extend(std::iter::repeat_n(b' ', fill));
    }
    out.extend_from_slice(s);
    if left {
        out.extend(std::iter::repeat_n(b' ', fill));
    }
}

fn c_format_int(spec: &[u8], v: i64) -> Vec<u8> {
    let fmt = sys::cstr(spec);
    let mut buf = vec![0u8; 512];
    // SAFETY: fmt is a single integer conversion with the `ll` modifier.
    let n = unsafe {
        libc::snprintf(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            fmt.as_ptr(),
            v as libc::c_longlong,
        )
    };
    buf.truncate(n.max(0) as usize);
    buf
}

fn c_format_float(spec: &[u8], v: f64) -> Vec<u8> {
    let fmt = sys::cstr(spec);
    let mut buf = vec![0u8; 512];
    // SAFETY: fmt is a single floating-point conversion.
    let n = unsafe { libc::snprintf(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), fmt.as_ptr(), v) };
    if n as usize >= buf.len() {
        buf = vec![0u8; n as usize + 1];
        // SAFETY: as above, with a big enough buffer.
        unsafe { libc::snprintf(buf.as_mut_ptr() as *mut libc::c_char, buf.len(), fmt.as_ptr(), v) };
    }
    buf.truncate(n.max(0) as usize);
    buf
}

/// Processes the format once. Returns `Err(())` to stop all output (`\c`
/// or an invalid directive).
fn format_once(sh: &Shell, fmt: &[u8], args: &mut Args, out: &mut Vec<u8>) -> Result<(), ()> {
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        if c == b'\\' {
            i += 1;
            let Some(&e) = fmt.get(i) else {
                out.push(b'\\');
                break;
            };
            i += 1;
            match e {
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'c' => return Err(()),
                b'f' => out.push(12),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'v' => out.push(11),
                b'e' => out.push(0x1b),
                b'\\' => out.push(e),
                b'0'..=b'7' => {
                    let mut v = (e - b'0') as u32;
                    let mut n = 1;
                    while n < 3 && i < fmt.len() && (b'0'..=b'7').contains(&fmt[i]) {
                        v = v * 8 + (fmt[i] - b'0') as u32;
                        i += 1;
                        n += 1;
                    }
                    out.push(v as u8);
                }
                _ => {
                    out.push(b'\\');
                    out.push(e);
                }
            }
            continue;
        }
        if c != b'%' {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if fmt.get(i) == Some(&b'%') {
            out.push(b'%');
            i += 1;
            continue;
        }
        let mut flags = Vec::new();
        while let Some(&f) = fmt.get(i) {
            if b"-+ #0".contains(&f) {
                flags.push(f);
                i += 1;
            } else {
                break;
            }
        }
        let mut width: Option<i64> = None;
        if fmt.get(i) == Some(&b'*') {
            width = Some(args.int(sh));
            i += 1;
        } else {
            let s = i;
            while fmt.get(i).is_some_and(|c| c.is_ascii_digit()) {
                i += 1;
            }
            if i > s {
                width = std::str::from_utf8(&fmt[s..i]).unwrap().parse().ok();
            }
        }
        let mut prec: Option<i64> = None;
        if fmt.get(i) == Some(&b'.') {
            i += 1;
            if fmt.get(i) == Some(&b'*') {
                prec = Some(args.int(sh));
                i += 1;
            } else {
                let s = i;
                while fmt.get(i).is_some_and(|c| c.is_ascii_digit()) {
                    i += 1;
                }
                prec = Some(std::str::from_utf8(&fmt[s..i]).unwrap().parse().unwrap_or(0));
            }
        }
        let Some(&conv) = fmt.get(i) else {
            sh.berr(
                b"printf",
                format!("{}: invalid directive", String::from_utf8_lossy(&fmt[start..])),
            );
            args.status = 2;
            return Err(());
        };
        i += 1;
        let mut left = flags.contains(&b'-');
        if width.is_some_and(|w| w < 0) {
            left = true;
            width = width.map(|w| -w);
        }
        let uwidth = width.map(|w| w as usize);
        let uprec = prec.filter(|&p| p >= 0).map(|p| p as usize);
        let mut spec = b"%".to_vec();
        spec.extend_from_slice(&flags);
        if let Some(w) = width {
            if left && !flags.contains(&b'-') {
                spec.push(b'-');
            }
            spec.extend_from_slice(w.to_string().as_bytes());
        }
        if let Some(p) = prec {
            spec.push(b'.');
            spec.extend_from_slice(p.to_string().as_bytes());
        }
        match conv {
            b's' => {
                let a = args.next().unwrap_or_default().to_vec();
                pad(out, &a, uwidth, uprec, left);
            }
            b'b' => {
                let a = args.next().unwrap_or_default().to_vec();
                let mut s = Vec::new();
                let stop = echo_escapes(&a, &mut s);
                pad(out, &s, uwidth, uprec, left);
                if stop {
                    return Err(());
                }
            }
            b'c' => {
                let a = args.next().unwrap_or_default();
                let s: Vec<u8> = a.first().copied().into_iter().collect();
                pad(out, &s, uwidth, None, left);
            }
            b'd' | b'i' | b'o' | b'u' | b'x' | b'X' => {
                let v = args.number(sh, matches!(conv, b'd' | b'i'));
                spec.extend_from_slice(b"ll");
                spec.push(conv);
                out.extend(c_format_int(&spec, v));
            }
            b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'a' | b'A' => {
                let v = args.float(sh);
                spec.push(conv);
                out.extend(c_format_float(&spec, v));
            }
            _ => {
                sh.berr(
                    b"printf",
                    format!("{}: invalid directive", String::from_utf8_lossy(&fmt[start..i])),
                );
                args.status = 2;
                return Err(());
            }
        }
    }
    Ok(())
}

pub fn printf(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let mut rest = &argv[1..];
    if rest.first().is_some_and(|a| a == b"--") {
        rest = &rest[1..];
    }
    let Some(fmt) = rest.first() else {
        sh.berr(&argv[0], "usage: printf format [arg ...]");
        return Ok(2);
    };
    let mut args = Args {
        args: &rest[1..],
        next: 0,
        status: 0,
    };
    let mut out = Vec::new();
    loop {
        let before = args.next;
        if format_once(sh, fmt, &mut args, &mut out).is_err() {
            break;
        }
        if args.next >= args.args.len() || args.next == before {
            break;
        }
    }
    sh.out(&out);
    Ok(args.status)
}
