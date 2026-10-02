//! The terminal's own colours, which a colour scheme can set: its text
//! colour, background, cursor colour and palette (OSC 10, 11, 12 and 4),
//! while the scheme is in use, putting back what they were when it no
//! longer is or the shell exits (DEVELOPING.md, Terminal colours).

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::shell::Shell;
use crate::style::Rgb;
use crate::sys;

/// A terminal colour by its OSC code: 10, 11 or 12, or 4 and the number of
/// the palette's colour (so the palette sorts first).
type Key = (u16, u8);

thread_local! {
    /// What the shell has set.
    static SET: RefCell<BTreeMap<Key, Rgb>> = const { RefCell::new(BTreeMap::new()) };
    /// What each colour was before the shell first set it, as the terminal
    /// told it; None if it didn't tell, and is then reset.
    static FOUND: RefCell<BTreeMap<Key, Option<Rgb>>> = const { RefCell::new(BTreeMap::new()) };
    /// What `update` last looked at: the styles' generation, the scheme in
    /// use, and whether colours were wanted at all; None to look again.
    static SEEN: RefCell<Option<(u64, Option<String>, bool)>> = const { RefCell::new(None) };
}

/// The OSC code of a key, as written in the sequences (`11`, `4;1`).
fn code(key: Key) -> String {
    match key {
        (4, n) => format!("4;{n}"),
        (c, _) => c.to_string(),
    }
}

/// The sequence that sets the colour `key`. The colour is written as
/// `#rrggbb`, which every terminal takes; some (Konsole) don't take the
/// `rgb:R/G/B` form that they answer with.
fn set_sequence(key: Key, (r, g, b): Rgb) -> String {
    format!("\x1b]{};#{r:02x}{g:02x}{b:02x}\x1b\\", code(key))
}

/// The colour of a terminal's answer, `rgb:R/G/B` with 1 to 4 hex digits
/// per channel (scaled to 8 bits).
fn parse_spec(spec: &[u8]) -> Option<Rgb> {
    let channels = std::str::from_utf8(spec).ok()?.strip_prefix("rgb:")?;
    let mut parts = channels.split('/');
    let mut out = [0u8; 3];
    for c in &mut out {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 4 {
            return None;
        }
        let max = (1u32 << (4 * p.len())) - 1;
        *c = ((u32::from_str_radix(p, 16).ok()? * 255 + max / 2) / max) as u8;
    }
    parts.next().is_none().then_some((out[0], out[1], out[2]))
}

/// The sequence that puts back the colour `key` as it was found, or resets
/// it to the terminal's default (OSC 110, 111, 112, or 104 for one of the
/// palette) if the terminal didn't tell what it was.
fn restore_sequence(key: Key, found: Option<Rgb>) -> String {
    match (found, key) {
        (Some(c), _) => set_sequence(key, c),
        (None, (4, n)) => format!("\x1b]104;{n}\x1b\\"),
        (None, (c, _)) => format!("\x1b]{}\x1b\\", c + 100),
    }
}

/// Whether the shell sets the terminal's colours: `terminal-colors` is on,
/// `$NO_COLOR` is empty or unset, and stdin and stderr are a terminal that
/// the line editor supports.
fn wanted(sh: &Shell) -> bool {
    sh.styles.terminal_colors() && sh.get_var(b"NO_COLOR").is_none_or(|v| v.is_empty()) && super::can_ask(sh)
}

/// The colours that the scheme in use gives, by key.
fn scheme_colors(sh: &Shell, scheme: Option<&str>) -> BTreeMap<Key, Rgb> {
    let mut out = BTreeMap::new();
    for (k, colors) in sh.styles.terminal(scheme) {
        match k.as_str() {
            "foreground" => out.insert((10, 0), colors[0]),
            "background" => out.insert((11, 0), colors[0]),
            "cursor" => out.insert((12, 0), colors[0]),
            _ => {
                for (n, &c) in colors.iter().enumerate() {
                    out.insert((4, n as u8), c);
                }
                None
            }
        };
    }
    out
}

/// Before each prompt: sets the terminal colours of the scheme in use, and
/// puts back those that the shell set but it doesn't give. Looks again only
/// when the styles, the scheme in use or whether colours are wanted have
/// changed. Before setting a colour for the first time, it asks the
/// terminal what it was; keys typed meanwhile start the command line.
pub fn update(sh: &mut Shell) {
    let on = wanted(sh);
    let scheme = crate::builtins::style::scheme_in_use(sh);
    let seen = (sh.styles.generation, scheme.clone(), on);
    if SEEN.with_borrow(|s| s.as_ref() == Some(&seen)) {
        return;
    }
    SEEN.set(Some(seen));
    let want = if on {
        scheme_colors(sh, scheme.as_deref())
    } else {
        BTreeMap::new()
    };
    let unknown: Vec<Key> = FOUND.with_borrow(|f| want.keys().filter(|k| !f.contains_key(k)).copied().collect());
    if !unknown.is_empty() {
        let codes: Vec<String> = unknown.iter().map(|&k| code(k)).collect();
        let answer = super::tty::ask(&codes);
        if let Some(a) = &answer
            && !a.typed.is_empty()
            && super::EDITS.get()
        {
            super::push_buffer(String::from_utf8_lossy(&a.typed).into_owned());
        }
        FOUND.with_borrow_mut(|f| {
            for (k, c) in unknown.iter().zip(&codes) {
                let told = answer.as_ref().and_then(|a| a.color(c)).and_then(parse_spec);
                f.insert(*k, told);
            }
        });
    }
    let mut out = String::new();
    SET.with_borrow_mut(|set| {
        FOUND.with_borrow(|found| {
            for (&k, _) in set.iter().filter(|(k, _)| !want.contains_key(k)) {
                out.push_str(&restore_sequence(k, found.get(&k).copied().flatten()));
            }
        });
        for (&k, &c) in &want {
            if set.get(&k) != Some(&c) {
                out.push_str(&set_sequence(k, c));
            }
        }
        *set = want;
    });
    if !out.is_empty() {
        sys::write_all(2, out.as_bytes());
    }
}

/// Puts back the colours that the shell set: when it exits, before `exec`,
/// and before it asks the terminal for its background (which would
/// otherwise be the scheme's). The next prompt sets them again, if the
/// shell goes on.
pub fn restore() {
    let mut out = String::new();
    SET.with_borrow_mut(|set| {
        FOUND.with_borrow(|found| {
            for &k in set.keys() {
                out.push_str(&restore_sequence(k, found.get(&k).copied().flatten()));
            }
        });
        set.clear();
    });
    SEEN.set(None);
    if !out.is_empty() {
        sys::write_all(2, out.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences() {
        assert_eq!(set_sequence((11, 0), (0x28, 0x28, 0x28)), "\x1b]11;#282828\x1b\\");
        assert_eq!(set_sequence((4, 9), (0xfb, 0x49, 0x34)), "\x1b]4;9;#fb4934\x1b\\");
        assert_eq!(
            restore_sequence((10, 0), Some((0xff, 0xff, 0xff))),
            "\x1b]10;#ffffff\x1b\\"
        );
        assert_eq!(restore_sequence((10, 0), None), "\x1b]110\x1b\\");
        assert_eq!(restore_sequence((12, 0), None), "\x1b]112\x1b\\");
        assert_eq!(restore_sequence((4, 3), None), "\x1b]104;3\x1b\\");
    }

    #[test]
    fn specs() {
        assert_eq!(parse_spec(b"rgb:ffff/ffff/ffff"), Some((255, 255, 255)));
        assert_eq!(parse_spec(b"rgb:1a1a/1b1b/2626"), Some((0x1a, 0x1b, 0x26)));
        assert_eq!(parse_spec(b"rgb:28/28/28"), Some((0x28, 0x28, 0x28)));
        assert_eq!(parse_spec(b"rgb:0/f/8"), Some((0, 255, 136)));
        assert_eq!(parse_spec(b"rgb:ff/ff"), None);
        assert_eq!(parse_spec(b"rgb:ff/ff/ff/ff"), None);
        assert_eq!(parse_spec(b"#282828"), None);
    }
}
