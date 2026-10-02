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
    /// told it (`rgb:...`); None if it didn't tell, and is then reset.
    static FOUND: RefCell<BTreeMap<Key, Option<Vec<u8>>>> = const { RefCell::new(BTreeMap::new()) };
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

/// The sequence that sets the colour `key`.
fn set_sequence(key: Key, (r, g, b): Rgb) -> String {
    format!("\x1b]{};rgb:{r:02x}/{g:02x}/{b:02x}\x1b\\", code(key))
}

/// The sequence that puts back the colour `key` as it was found, or resets
/// it to the terminal's default (OSC 110, 111, 112, or 104 for one of the
/// palette) if the terminal didn't tell what it was.
fn restore_sequence(key: Key, found: Option<&[u8]>) -> String {
    match (found, key) {
        (Some(spec), _) => format!("\x1b]{};{}\x1b\\", code(key), String::from_utf8_lossy(spec)),
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
                // Only what can be given back as it is.
                let told = answer
                    .as_ref()
                    .and_then(|a| a.color(c))
                    .filter(|v| v.starts_with(b"rgb:"));
                f.insert(*k, told.map(<[u8]>::to_vec));
            }
        });
    }
    let mut out = String::new();
    SET.with_borrow_mut(|set| {
        FOUND.with_borrow(|found| {
            for (&k, _) in set.iter().filter(|(k, _)| !want.contains_key(k)) {
                out.push_str(&restore_sequence(k, found.get(&k).cloned().flatten().as_deref()));
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
                out.push_str(&restore_sequence(k, found.get(&k).cloned().flatten().as_deref()));
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
        assert_eq!(set_sequence((11, 0), (0x28, 0x28, 0x28)), "\x1b]11;rgb:28/28/28\x1b\\");
        assert_eq!(set_sequence((4, 9), (0xfb, 0x49, 0x34)), "\x1b]4;9;rgb:fb/49/34\x1b\\");
        assert_eq!(
            restore_sequence((10, 0), Some(b"rgb:ffff/ffff/ffff")),
            "\x1b]10;rgb:ffff/ffff/ffff\x1b\\"
        );
        assert_eq!(restore_sequence((10, 0), None), "\x1b]110\x1b\\");
        assert_eq!(restore_sequence((12, 0), None), "\x1b]112\x1b\\");
        assert_eq!(restore_sequence((4, 3), None), "\x1b]104;3\x1b\\");
    }
}
