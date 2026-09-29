//! Conversion between shell bytes and Rhai strings (DEVELOPING.md).
//!
//! Rhai strings hold UTF-8 only. Valid UTF-8 passes through unchanged, and
//! each byte `b` of an invalid sequence becomes the code point U+10FF00 + `b`
//! (as Python's `surrogateescape` does with lone surrogates), so that shell
//! data round-trips exactly.

const ESCAPE_BASE: u32 = 0x10FF00;

/// Shell bytes to a Rhai string.
pub fn to_str(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len());
    for chunk in b.utf8_chunks() {
        out.push_str(chunk.valid());
        for &byte in chunk.invalid() {
            // Only bytes 0x80-0xFF can be invalid, so this is U+10FF80-U+10FFFF.
            out.push(char::from_u32(ESCAPE_BASE + byte as u32).unwrap());
        }
    }
    out
}

/// A Rhai string to shell bytes: escaped bytes become bytes again, and
/// everything else is encoded as UTF-8.
pub fn to_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c as u32 {
            n @ 0x10FF80..=0x10FFFF => out.push((n - ESCAPE_BASE) as u8),
            _ => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_unchanged() {
        assert_eq!(to_str("héllo ✓".as_bytes()), "héllo ✓");
        assert_eq!(to_bytes("héllo ✓"), "héllo ✓".as_bytes());
    }

    #[test]
    fn invalid_bytes_round_trip() {
        let cases: &[&[u8]] = &[
            b"\xff",
            b"a\x80b",
            b"\xc3",
            b"\xe2\x9c",
            b"\xc3\xa9\xff\xfe",
            &[0x80, 0xc0, 0xaf],
        ];
        for &b in cases {
            assert_eq!(to_bytes(&to_str(b)), b);
        }
        assert_eq!(to_str(b"a\xffb"), "a\u{10FFFF}b");
    }

    #[test]
    fn all_bytes_round_trip() {
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(to_bytes(&to_str(&all)), all);
    }

    #[test]
    fn odd_sequences_round_trip() {
        let pieces: [u8; 9] = [0x00, b'a', 0x7f, 0x80, 0xbf, 0xc3, 0xa9, 0xe2, 0xff];
        let mut idx = [0usize; 5];
        for len in 0..=5 {
            for n in 0..pieces.len().pow(len as u32) {
                let mut m = n;
                for slot in idx.iter_mut().take(len) {
                    *slot = m % pieces.len();
                    m /= pieces.len();
                }
                let b: Vec<u8> = idx[..len].iter().map(|&i| pieces[i]).collect();
                assert_eq!(to_bytes(&to_str(&b)), b);
            }
        }
    }

    #[test]
    fn escape_characters_in_a_string() {
        // Whatever a script builds, converting it doesn't panic.
        let s: String = ['\u{10FF7F}', '\u{10FF80}', '\u{10FFFF}', '\u{0}', 'é']
            .iter()
            .collect();
        let _ = to_bytes(&s);
    }
}
