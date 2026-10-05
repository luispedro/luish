//! Colours for what `plugin` prints on standard output, from the `plugin.*`
//! styles of the colour scheme in use (`crate::style`). There are none when
//! standard output isn't a terminal or `$NO_COLOR` is set, so that what
//! scripts read stays plain text.

use crate::shell::Shell;
use crate::style::Role;

/// What a piece of text is.
#[derive(Clone, Copy)]
pub enum Kind {
    /// A plugin's or a source's name.
    Name,
    /// Something that worked, or is as it should be.
    Ok,
    /// Something changed, or can.
    Update,
    /// Something that needs attention, though it isn't an error.
    Warn,
    /// An error.
    Error,
    /// Detail: hashes, links, hints.
    Dim,
}

const KINDS: [(Kind, Role); 6] = [
    (Kind::Name, Role::of("plugin.name")),
    (Kind::Ok, Role::of("plugin.ok")),
    (Kind::Update, Role::of("plugin.update")),
    (Kind::Warn, Role::of("plugin.warn")),
    (Kind::Error, Role::of("plugin.error")),
    (Kind::Dim, Role::of("plugin.dim")),
];

/// The escape sequences for the kinds, or none for plain text.
pub struct Ui {
    sgr: Option<[String; KINDS.len()]>,
}

impl Ui {
    /// The colours for standard output.
    pub fn new(sh: &Shell) -> Ui {
        Ui {
            sgr: crate::builtins::style::stdout_sgr(sh, KINDS.map(|(_, role)| role)),
        }
    }

    /// No colours.
    pub fn plain() -> Ui {
        Ui { sgr: None }
    }

    /// `text` in the style of `kind`.
    pub fn paint(&self, kind: Kind, text: &str) -> String {
        let on = self.sgr.as_ref().map(|s| &s[kind as usize]).filter(|s| !s.is_empty());
        match on {
            Some(on) => format!("{on}{text}\x1b[m"),
            None => text.to_string(),
        }
    }

    /// A line made of `parts` (each in its kind) separated by spaces, and a
    /// newline.
    pub fn line(&self, parts: &[(Kind, &str)]) -> String {
        let words: Vec<String> = (parts.iter())
            .filter(|(_, t)| !t.is_empty())
            .map(|(kind, text)| self.paint(*kind, text))
            .collect();
        words.join(" ") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_is_text() {
        let ui = Ui::plain();
        assert_eq!(ui.paint(Kind::Ok, "x"), "x");
        assert_eq!(ui.line(&[(Kind::Name, "a"), (Kind::Dim, "b")]), "a b\n");
        assert_eq!(ui.line(&[(Kind::Name, "a"), (Kind::Dim, "")]), "a\n");
    }

    #[test]
    fn kinds_are_in_order() {
        for (i, (k, _)) in KINDS.iter().enumerate() {
            assert_eq!(*k as usize, i);
        }
    }

    #[test]
    fn painted() {
        let ui = Ui {
            sgr: Some([
                "\x1b[1m".into(),
                "\x1b[32m".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]),
        };
        assert_eq!(ui.paint(Kind::Ok, "x"), "\x1b[32mx\x1b[m");
        assert_eq!(ui.paint(Kind::Update, "x"), "x");
    }
}
