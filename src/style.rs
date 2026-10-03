//! Styles: the colours and attributes of the line editor's highlighting,
//! completion menu and autosuggestions (and later of prompts), by name
//! (DEVELOPING.md, Styles).
//!
//! A style's value is words separated by spaces: a foreground colour, a
//! background colour (`bg:COLOUR`), attributes (`bold`, and `no-bold` to
//! turn one off), the kind of underline (`undercurl` and the like, which
//! underline too) and its colour (`ul:COLOUR`), `plain` (the terminal's
//! defaults: inherit nothing) and `sgr:PARAMS` (raw SGR parameters). Colours are spelt as in prompts'
//! `%F{...}`: a name, `bright-NAME`, `default`, 0 to 255 or `#rrggbb`.
//!
//! Names are dotted, and a name falls back to its parent field by field
//! (`command.alias` takes what it doesn't set from `command`), as in helix.
//! The names in [`ROLES`] are luish's own and validated; other names are
//! free (for prompts and plugins), unless they start with a role's first
//! component, so that typos are caught.
//!
//! Values come in layers: plugins' defaults for their own names, then the
//! colour scheme in use (with the schemes it inherits from), then the
//! user's settings. A layer replaces a name's value as a whole, and the
//! fallback to parents happens after the layers are merged, so a scheme's
//! `command.unknown` still wins over the user's `command`. The scheme is
//! chosen by name, or as a pair chosen by whether the terminal's background
//! is dark or light ([`background`]); the built-in schemes are
//! `default-dark` and `default-light`, chosen as a pair by default.
//!
//! A scheme can also give the terminal's own colours (its background, text
//! and cursor colours and the 16 of its palette), as `terminal.KEY`
//! ([`TERMINAL_KEYS`]), which the shell sets while the scheme is in use
//! (`interactive/termcolors.rs`), unless `--terminal-colors off`.

use std::collections::BTreeMap;

/// luish's own names: the highlighting roles, the modifiers, and the line
/// editor's other uses.
pub const ROLES: &[&str] = &[
    "command",
    "command.builtin",
    "command.function",
    "command.alias",
    "command.external",
    "command.precommand",
    "command.directory",
    "command.history",
    "command.unknown",
    "keyword",
    "arg",
    "arg.option",
    "arg.subcommand",
    "string",
    "string.single",
    "string.double",
    "string.dollar",
    "string.heredoc",
    "string.escape",
    "var",
    "var.special",
    "var.array",
    "var.exported",
    "var.readonly",
    "var.unset",
    "subst",
    "subst.command",
    "subst.process",
    "subst.arith",
    "expand",
    "expand.tilde",
    "expand.brace",
    "expand.glob",
    "op",
    "op.control",
    "op.pipe",
    "redir",
    "redir.fd",
    "assign",
    "comment",
    "error",
    "path",
    "path.prefix",
    "match",
    "menu",
    "menu.selected",
    "menu.description",
    "suggestion",
];

const _: () = assert!(ROLES.len() < u8::MAX as usize);

/// A name in [`ROLES`], by its position, or none.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Role(u8);

impl Role {
    pub const NONE: Role = Role(u8::MAX);

    /// The role `name`; a name not in [`ROLES`] fails to compile (in a
    /// constant).
    pub const fn of(name: &str) -> Role {
        let (name, mut i) = (name.as_bytes(), 0);
        while i < ROLES.len() {
            let r = ROLES[i].as_bytes();
            if r.len() == name.len() {
                let mut k = 0;
                while k < r.len() && r[k] == name[k] {
                    k += 1;
                }
                if k == r.len() {
                    return Role(i as u8);
                }
            }
            i += 1;
        }
        panic!("not a role")
    }

    /// The position in [`ROLES`], or None for no role.
    pub fn index(self) -> Option<usize> {
        (self != Role::NONE).then_some(self.0 as usize)
    }
}

impl Default for Role {
    fn default() -> Role {
        Role::NONE
    }
}

/// A built-in scheme: its name, the scheme it inherits from, and its
/// values.
type Builtin = (
    &'static str,
    Option<&'static str>,
    &'static [(&'static str, &'static str)],
);

/// The built-in schemes.
const BUILTIN: &[Builtin] = &[
    (
        "default-dark",
        None,
        &[
            ("keyword", "bold blue"),
            ("command", "green"),
            ("command.function", "bold green"),
            ("command.alias", "italic green"),
            ("command.unknown", "bold red"),
            ("string", "yellow"),
            ("var", "cyan"),
            ("var.exported", "bold cyan"),
            ("var.unset", "dim cyan"),
            ("subst", "magenta"),
            ("expand", "blue"),
            ("op", "bold"),
            ("redir", "bold"),
            ("comment", "bright-black"),
            ("assign", "blue"),
            ("error", "red underline"),
            ("path", "underline"),
            ("menu.selected", "reverse"),
            ("menu.description", "bright-black"),
            ("suggestion", "bright-black"),
        ],
    ),
    // Yellow is hard to read on a light background.
    ("default-light", Some("default-dark"), &[("string", "136")]),
];

/// The scheme pair used when none is chosen.
const DEFAULT_PAIR: (&str, &str) = ("default-dark", "default-light");

/// How deep `inherits` chains are followed (a cycle can't be made, but a
/// chain through schemes defined later is only checked as it is used).
const MAX_INHERITS: usize = 16;

/// The 8 colours, by name and number.
const COLORS: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/// The attributes: (name, SGR parameter).
const ATTRS: [(&str, u8); 7] = [
    ("bold", 1),
    ("dim", 2),
    ("italic", 3),
    ("underline", 4),
    ("blink", 5),
    ("reverse", 7),
    ("strike", 9),
];

/// The kinds of underline other than the straight one, as vim names them:
/// (name, the SGR subparameter of `4`).
const UNDERLINES: [(&str, u8); 4] = [
    ("underdouble", 2),
    ("undercurl", 3),
    ("underdotted", 4),
    ("underdashed", 5),
];

/// The bit of an attribute (by name, as in [`ATTRS`]) in [`Style::on`] and
/// [`Style::off`].
pub fn attr_bit(name: &str) -> u8 {
    ATTRS.iter().position(|a| a.0 == name).map_or(0, |i| 1 << i)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    /// The terminal's own colour.
    Default,
    /// A palette colour: 0 to 7, 8 to 15 (bright), or 16 to 255.
    Index(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// A colour name (`red`, `bright-red`, `default`), a number from 0 to
    /// 255, or `#rrggbb` (or `#rgb`).
    pub fn parse(spec: &[u8]) -> Option<Color> {
        if spec == b"default" {
            return Some(Color::Default);
        }
        let named = |s: &[u8]| COLORS.iter().position(|&c| c.as_bytes() == s).map(|i| i as u8);
        if let Some(i) = named(spec) {
            return Some(Color::Index(i));
        }
        if let Some(i) = spec.strip_prefix(b"bright-").and_then(named) {
            return Some(Color::Index(i + 8));
        }
        if !spec.is_empty() && spec.len() <= 3 && spec.iter().all(u8::is_ascii_digit) {
            let n: u16 = std::str::from_utf8(spec).ok()?.parse().ok()?;
            return u8::try_from(n).ok().map(Color::Index);
        }
        let (r, g, b) = rgb(spec)?;
        Some(Color::Rgb(r, g, b))
    }

    /// Appends the SGR parameters for the colour as the foreground, or the
    /// background if `bg`.
    pub fn sgr(self, bg: bool, out: &mut String) {
        let base = if bg { 40 } else { 30 };
        let s = match self {
            Color::Default => format!("{}", base + 9),
            Color::Index(n @ 0..8) => format!("{}", base + n as u32),
            Color::Index(n @ 8..16) => format!("{}", base + 60 + n as u32 - 8),
            Color::Index(n) => format!("{};5;{n}", base + 8),
            Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        };
        out.push_str(&s);
    }

    /// Appends the SGR parameters for the colour as the underline's (58,
    /// which has no short form for the first 16).
    fn sgr_underline(self, out: &mut String) {
        let s = match self {
            Color::Default => "59".to_owned(),
            Color::Index(n) => format!("58;5;{n}"),
            Color::Rgb(r, g, b) => format!("58;2;{r};{g};{b}"),
        };
        out.push_str(&s);
    }

    /// The colour as it would be written.
    fn text(self) -> String {
        match self {
            Color::Default => "default".to_owned(),
            Color::Index(n @ 0..8) => COLORS[n as usize].to_owned(),
            Color::Index(n @ 8..16) => format!("bright-{}", COLORS[n as usize - 8]),
            Color::Index(n) => n.to_string(),
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

/// `#rrggbb` or `#rgb`.
pub fn rgb(spec: &[u8]) -> Option<(u8, u8, u8)> {
    let hex = std::str::from_utf8(spec.strip_prefix(b"#")?).ok()?;
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |s: &str| u8::from_str_radix(s, 16).ok();
    match hex.len() {
        6 => Some((v(&hex[0..2])?, v(&hex[2..4])?, v(&hex[4..6])?)),
        3 => Some((v(&hex[0..1])? * 17, v(&hex[1..2])? * 17, v(&hex[2..3])? * 17)),
        _ => None,
    }
}

/// A colour as red, green and blue.
pub type Rgb = (u8, u8, u8);

/// The terminal's colours that a scheme can set, as `terminal.KEY`: its
/// text colour, background and cursor (OSC 10, 11 and 12), and its palette
/// (OSC 4), from colour 0.
pub const TERMINAL_KEYS: [&str; 4] = ["foreground", "background", "cursor", "palette"];

/// The most colours of the palette that a scheme sets: the 16 of ANSI.
pub const PALETTE_MAX: usize = 16;

/// The prefix of the names of the terminal's colours in a scheme.
pub const TERMINAL_PREFIX: &str = "terminal.";

/// The key of a name of the terminal's colours (`background` for
/// `terminal.background`), if it is one.
pub fn terminal_key(name: &str) -> Option<&str> {
    name.strip_prefix(TERMINAL_PREFIX)
}

/// Checks the key of a terminal colour.
pub fn check_terminal_key(key: &str) -> Result<(), String> {
    match TERMINAL_KEYS.contains(&key) {
        true => Ok(()),
        false => Err(format!("not a terminal colour (expected {})", TERMINAL_KEYS.join(", "))),
    }
}

/// Parses the value of the terminal's colour `key`: one `#rrggbb` (or
/// `#rgb`), or for `palette`, up to [`PALETTE_MAX`] of them; the words may
/// be one argument or several.
pub fn parse_terminal<S: AsRef<[u8]>>(key: &str, args: &[S]) -> Result<Vec<Rgb>, String> {
    check_terminal_key(key)?;
    let words = args.iter().flat_map(|a| a.as_ref().split(u8::is_ascii_whitespace));
    let mut out = Vec::new();
    for w in words.filter(|w| !w.is_empty()) {
        let c = rgb(w).ok_or_else(|| format!("bad colour: {} (expected #rrggbb)", String::from_utf8_lossy(w)))?;
        out.push(c);
    }
    let max = if key == "palette" { PALETTE_MAX } else { 1 };
    match out.len() {
        0 => Err("missing colour".to_owned()),
        n if n > max => Err(format!("{n} colours (at most {max})")),
        _ => Ok(out),
    }
}

/// A terminal colour's value as it would be written.
pub fn terminal_text(colors: &[Rgb]) -> String {
    let words: Vec<String> = colors
        .iter()
        .map(|&(r, g, b)| format!("#{r:02x}{g:02x}{b:02x}"))
        .collect();
    words.join(" ")
}

/// A style's value. A field left out (`None`, or an attribute in neither
/// `on` nor `off`) comes from the parent name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    /// Attributes turned on, and off, as bits by their index in `ATTRS`.
    pub on: u8,
    pub off: u8,
    /// The kind of underline (a subparameter of SGR 4, as in
    /// [`UNDERLINES`]), with `underline` on.
    pub under: Option<u8>,
    /// The underline's colour.
    pub ul: Option<Color>,
    /// `plain`: nothing comes from the parent.
    pub plain: bool,
    /// `sgr:PARAMS`, written after the rest.
    pub raw: Option<String>,
}

impl Style {
    /// Parses the words of a value (an argument may hold several).
    pub fn parse<S: AsRef<[u8]>>(args: &[S]) -> Result<Style, String> {
        let mut s = Style::default();
        let words = args.iter().flat_map(|a| a.as_ref().split(u8::is_ascii_whitespace));
        for w in words.filter(|w| !w.is_empty()) {
            let shown = || String::from_utf8_lossy(w).into_owned();
            let attr = |name: &[u8]| ATTRS.iter().position(|a| a.0.as_bytes() == name);
            if w == b"plain" {
                s.plain = true;
            } else if let Some(i) = attr(w) {
                s.on |= 1 << i;
                s.off &= !(1 << i);
            } else if let Some(i) = w.strip_prefix(b"no-").and_then(attr) {
                s.off |= 1 << i;
                s.on &= !(1 << i);
            } else if let Some(&(_, kind)) = UNDERLINES.iter().find(|u| u.0.as_bytes() == w) {
                s.under = Some(kind);
                s.on |= attr_bit("underline");
                s.off &= !attr_bit("underline");
            } else if let Some(c) = w.strip_prefix(b"ul:") {
                let c = Color::parse(c).ok_or_else(|| format!("bad colour: {}", shown()))?;
                if s.ul.replace(c).is_some() {
                    return Err(format!("two underline colours: {}", shown()));
                }
            } else if let Some(c) = w.strip_prefix(b"bg:") {
                let c = Color::parse(c).ok_or_else(|| format!("bad colour: {}", shown()))?;
                if s.bg.replace(c).is_some() {
                    return Err(format!("two background colours: {}", shown()));
                }
            } else if let Some(p) = w.strip_prefix(b"sgr:") {
                if p.is_empty() || !p.iter().all(|&c| c.is_ascii_digit() || c == b';' || c == b':') {
                    return Err(format!("bad SGR parameters: {}", shown()));
                }
                s.raw = Some(String::from_utf8_lossy(p).into_owned());
            } else if let Some(c) = Color::parse(w) {
                if s.fg.replace(c).is_some() {
                    return Err(format!("two colours: {}", shown()));
                }
            } else {
                return Err(format!("bad style: {}", shown()));
            }
        }
        Ok(s)
    }

    /// The value as it would be written (`plain` if it sets nothing).
    pub fn text(&self) -> String {
        let mut words = Vec::new();
        if self.plain {
            words.push("plain".to_owned());
        }
        for (i, (name, _)) in ATTRS.iter().enumerate() {
            if self.on & (1 << i) != 0 {
                let under = self.under.filter(|_| *name == "underline");
                let under = under.and_then(|k| UNDERLINES.iter().find(|u| u.1 == k));
                words.push(under.map_or(*name, |u| u.0).to_owned());
            }
        }
        for (i, (name, _)) in ATTRS.iter().enumerate() {
            if self.off & (1 << i) != 0 {
                words.push(format!("no-{name}"));
            }
        }
        if let Some(c) = self.fg {
            words.push(c.text());
        }
        if let Some(c) = self.bg {
            words.push(format!("bg:{}", c.text()));
        }
        if let Some(c) = self.ul {
            words.push(format!("ul:{}", c.text()));
        }
        if let Some(r) = &self.raw {
            words.push(format!("sgr:{r}"));
        }
        if words.is_empty() {
            return "plain".to_owned();
        }
        words.join(" ")
    }

    /// Fills the fields this style leaves out from `parent`, unless it is
    /// `plain`; after a `plain` parent, nothing more is inherited.
    fn inherit(&mut self, parent: &Style) {
        if self.plain {
            return;
        }
        self.fg = self.fg.or(parent.fg);
        self.bg = self.bg.or(parent.bg);
        self.ul = self.ul.or(parent.ul);
        // The kind goes with the parent's underline, unless this style
        // sets its own underline.
        if self.on & attr_bit("underline") == 0 {
            self.under = self.under.or(parent.under);
        }
        self.on |= parent.on & !self.off;
        self.off |= parent.off & !self.on;
        if self.raw.is_none() {
            self.raw.clone_from(&parent.raw);
        }
        self.plain = parent.plain;
    }

    /// This style with a modifier's on top: what `modifier` sets replaces
    /// this style's.
    pub fn add(&self, modifier: &Style) -> Style {
        let mut s = modifier.clone();
        s.inherit(self);
        s
    }

    /// The SGR parameters (without `ESC [` and `m`); empty for the
    /// terminal's defaults.
    pub fn sgr(&self) -> String {
        let mut out = String::new();
        let sep = |out: &mut String| {
            if !out.is_empty() {
                out.push(';');
            }
        };
        for (i, (name, code)) in ATTRS.iter().enumerate() {
            if self.on & (1 << i) != 0 {
                sep(&mut out);
                out.push_str(&code.to_string());
                if let Some(k) = self.under.filter(|_| *name == "underline") {
                    out.push_str(&format!(":{k}"));
                }
            }
        }
        if let Some(c) = self.fg {
            sep(&mut out);
            c.sgr(false, &mut out);
        }
        if let Some(c) = self.bg {
            sep(&mut out);
            c.sgr(true, &mut out);
        }
        if let Some(c) = self.ul {
            sep(&mut out);
            c.sgr_underline(&mut out);
        }
        if let Some(r) = &self.raw {
            sep(&mut out);
            out.push_str(r);
        }
        out
    }
}

/// The parent of a name (`command` for `command.alias`).
pub fn parent(name: &str) -> Option<&str> {
    name.rfind('.').map(|i| &name[..i])
}

/// Checks a style name: one of [`ROLES`], or a name that isn't in their
/// namespaces (whose first component isn't a role's, and isn't close to a
/// top-level role).
pub fn check_name(name: &str) -> Result<(), String> {
    if ROLES.contains(&name) {
        return Ok(());
    }
    let valid = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    if name == "colorscheme" || !name.split('.').all(valid) {
        return Err(format!("bad style name: {name}"));
    }
    let first = name.split('.').next().unwrap_or(name);
    match first {
        "terminal" => {
            return Err(format!(
                "{name}: not a style: a colour scheme's (style -s SCHEME {name} ...)"
            ));
        }
        "terminal-colors" | "terminal-colours" => {
            return Err(format!(
                "{name}: not a style: a setting (style --terminal-colors on|off)"
            ));
        }
        _ => {}
    }
    let reserved = ROLES.iter().any(|r| r.split('.').next() == Some(first));
    let max = if name.len() <= 3 { 1 } else { 2 };
    let near = (ROLES.iter())
        .filter(|r| reserved || !r.contains('.'))
        .map(|r| (crate::prompt::distance(name.as_bytes(), r.as_bytes()), r))
        .filter(|&(d, _)| d <= max)
        .min_by_key(|&(d, _)| d)
        .map(|(_, r)| r);
    match (reserved, near) {
        (_, Some(n)) if !name.contains('.') || reserved => Err(format!("unknown style: {name}; did you mean {n}?")),
        (true, _) => Err(format!("unknown style: {name}")),
        _ => Ok(()),
    }
}

/// Checks the name of a colour scheme.
pub fn check_scheme_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'));
    if ok {
        Ok(())
    } else {
        Err(format!("bad colour scheme name: {name}"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Background {
    Dark,
    Light,
}

/// Whether the terminal's background is dark or light, from
/// `$LUISH_BACKGROUND` (`dark` or `light`) or else `$COLORFGBG` (`FG;BG`,
/// or `FG;X;BG`, where a BG of 0 to 6 or 8 is dark, and 7 or 9 to 15
/// light), as given; None if neither tells.
pub fn background(luish: Option<&[u8]>, colorfgbg: Option<&[u8]>) -> Option<Background> {
    match luish {
        Some(b"dark") => return Some(Background::Dark),
        Some(b"light") => return Some(Background::Light),
        _ => {}
    }
    let last = colorfgbg?.rsplit(|&c| c == b';').next()?;
    match std::str::from_utf8(last).ok()?.parse::<u8>().ok()? {
        0..=6 | 8 => Some(Background::Dark),
        7 | 9..=15 => Some(Background::Light),
        _ => None,
    }
}

/// The colour scheme chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// None at all (`style --clear`).
    Nothing,
    One(String),
    /// By the background, with the scheme for when it is unknown (the dark
    /// one if not given).
    Pair {
        dark: String,
        light: String,
        default: Option<String>,
    },
}

impl Choice {
    /// The scheme for the background `bg`.
    pub fn scheme(&self, bg: Option<Background>) -> Option<&str> {
        match self {
            Choice::Nothing => None,
            Choice::One(n) => Some(n),
            Choice::Pair { dark, light, default } => Some(match bg {
                Some(Background::Dark) => dark,
                Some(Background::Light) => light,
                None => default.as_ref().unwrap_or(dark),
            }),
        }
    }

    /// The names of the schemes, as `style -c` takes them.
    pub fn names(&self) -> Vec<&str> {
        match self {
            Choice::Nothing => Vec::new(),
            Choice::One(n) => vec![n],
            Choice::Pair { dark, light, default } => [Some(dark), Some(light), default.as_ref()]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect(),
        }
    }
}

/// A colour scheme defined by the user or a plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scheme {
    pub inherits: Option<String>,
    pub values: BTreeMap<String, Style>,
    /// The terminal's colours, by their key in [`TERMINAL_KEYS`].
    pub terminal: BTreeMap<String, Vec<Rgb>>,
}

/// Where a name's value comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    User,
    Scheme(String),
    Plugin,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Origin::User => write!(f, "set by the user"),
            Origin::Scheme(s) => write!(f, "scheme {s}"),
            Origin::Plugin => write!(f, "plugin default"),
        }
    }
}

/// All the styles. Empty (and allocating nothing) until something is set,
/// so it costs nothing in shells that don't use it.
#[derive(Debug, Default)]
pub struct Styles {
    /// The schemes defined (a built-in one only if redefined).
    schemes: BTreeMap<String, Scheme>,
    /// Plugins' defaults (`[style]` in `plugin.toml`).
    defaults: BTreeMap<String, Style>,
    /// The user's settings, over the scheme.
    user: BTreeMap<String, Style>,
    /// None for the built-in pair.
    choice: Option<Choice>,
    /// `terminal-colors = false`: the schemes' terminal colours aren't set.
    no_terminal_colors: bool,
    /// Changed by every change, so that the line editor knows when to
    /// resolve the styles again.
    pub generation: u64,
}

impl Styles {
    fn changed(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// The scheme chosen.
    pub fn choice(&self) -> Choice {
        self.choice.clone().unwrap_or_else(|| Choice::Pair {
            dark: DEFAULT_PAIR.0.to_owned(),
            light: DEFAULT_PAIR.1.to_owned(),
            default: None,
        })
    }

    /// Chooses the scheme. Schemes that aren't defined yet are allowed (a
    /// plugin loaded later may define them).
    pub fn set_choice(&mut self, c: Choice) {
        let builtin = Choice::Pair {
            dark: DEFAULT_PAIR.0.to_owned(),
            light: DEFAULT_PAIR.1.to_owned(),
            default: None,
        };
        self.choice = (c != builtin).then_some(c);
        self.changed();
    }

    /// Whether the scheme `name` exists (defined or built in).
    pub fn has_scheme(&self, name: &str) -> bool {
        self.schemes.contains_key(name) || BUILTIN.iter().any(|b| b.0 == name)
    }

    /// The names of the schemes, built-in ones first.
    pub fn scheme_names(&self) -> Vec<&str> {
        let mut out: Vec<&str> = BUILTIN.iter().map(|b| b.0).collect();
        for n in self.schemes.keys() {
            if !out.contains(&n.as_str()) {
                out.push(n);
            }
        }
        out
    }

    /// The scheme `name`, with built-in ones made into a `Scheme`.
    pub fn scheme(&self, name: &str) -> Option<Scheme> {
        if let Some(s) = self.schemes.get(name) {
            return Some(s.clone());
        }
        let (_, inherits, values) = BUILTIN.iter().find(|b| b.0 == name)?;
        Some(Scheme {
            inherits: inherits.map(str::to_owned),
            values: (values.iter())
                .map(|(k, v)| ((*k).to_owned(), Style::parse(&[v]).unwrap_or_default()))
                .collect(),
            terminal: BTreeMap::new(),
        })
    }

    /// The scheme `name` for changing, defined (as a copy of the built-in
    /// one of that name, if there is one) if it isn't.
    fn scheme_mut(&mut self, name: &str) -> &mut Scheme {
        if !self.schemes.contains_key(name) {
            let s = self.scheme(name).unwrap_or_default();
            self.schemes.insert(name.to_owned(), s);
        }
        self.changed();
        self.schemes.get_mut(name).expect("inserted")
    }

    /// Defines the scheme `name`, replacing any definition it had (as a
    /// table in `config.toml` or `plugin.toml` does).
    pub fn replace_scheme(&mut self, name: &str, scheme: Scheme) {
        self.schemes.insert(name.to_owned(), scheme);
        self.changed();
    }

    /// Sets what the scheme `name` inherits from, defining it, empty, if
    /// it isn't (a built-in one isn't copied: this is how the saved state
    /// starts each scheme). An error if that would make a cycle.
    pub fn set_inherits(&mut self, name: &str, parent: Option<&str>) -> Result<(), String> {
        if let Some(p) = parent {
            let mut cur = Some(p.to_owned());
            for _ in 0..MAX_INHERITS {
                let Some(c) = cur else { break };
                if c == name {
                    return Err(format!("{name}: inherits from itself (through {p})"));
                }
                cur = self.scheme(&c).and_then(|s| s.inherits);
            }
        }
        self.schemes.entry(name.to_owned()).or_default().inherits = parent.map(str::to_owned);
        self.changed();
        Ok(())
    }

    /// Removes the scheme `name` (a built-in one comes back as it was).
    pub fn delete_scheme(&mut self, name: &str) -> bool {
        let found = self.schemes.remove(name).is_some();
        self.changed();
        found
    }

    pub fn set_in_scheme(&mut self, scheme: &str, name: &str, value: Style) {
        self.scheme_mut(scheme).values.insert(name.to_owned(), value);
    }

    /// Removes the style `name` from a scheme, or the terminal colour, for
    /// a name `terminal.KEY`.
    pub fn remove_from_scheme(&mut self, scheme: &str, name: &str) {
        if self.has_scheme(scheme) {
            let s = self.scheme_mut(scheme);
            match terminal_key(name) {
                Some(key) => drop(s.terminal.remove(key)),
                None => drop(s.values.remove(name)),
            }
        }
    }

    /// Sets the terminal colour `key` (in [`TERMINAL_KEYS`]) of a scheme.
    pub fn set_terminal_in_scheme(&mut self, scheme: &str, key: &str, colors: Vec<Rgb>) {
        self.scheme_mut(scheme).terminal.insert(key.to_owned(), colors);
    }

    /// Whether the schemes' terminal colours are set (`terminal-colors`).
    pub fn terminal_colors(&self) -> bool {
        !self.no_terminal_colors
    }

    pub fn set_terminal_colors(&mut self, on: bool) {
        self.no_terminal_colors = !on;
        self.changed();
    }

    /// The terminal colours of the scheme `scheme`: each key from the first
    /// scheme of the chain that gives it.
    pub fn terminal(&self, scheme: Option<&str>) -> BTreeMap<String, Vec<Rgb>> {
        let mut out = BTreeMap::new();
        for (_, s) in self.chain(scheme) {
            for (k, v) in s.terminal {
                out.entry(k).or_insert(v);
            }
        }
        out
    }

    pub fn set_user(&mut self, name: &str, value: Style) {
        self.user.insert(name.to_owned(), value);
        self.changed();
    }

    pub fn remove_user(&mut self, name: &str) {
        self.user.remove(name);
        self.changed();
    }

    pub fn set_default(&mut self, name: &str, value: Style) {
        self.defaults.insert(name.to_owned(), value);
        self.changed();
    }

    pub fn remove_default(&mut self, name: &str) {
        self.defaults.remove(name);
        self.changed();
    }

    /// `style --clear`: no settings of the user's, and no scheme.
    pub fn clear(&mut self) {
        self.user.clear();
        self.choice = Some(Choice::Nothing);
        self.changed();
    }

    /// The chain of schemes from `scheme` through what each inherits
    /// from, as far as they exist.
    fn chain(&self, scheme: Option<&str>) -> Vec<(String, Scheme)> {
        let mut out = Vec::new();
        let mut cur = scheme.map(str::to_owned);
        while let Some(name) = cur {
            if out.len() == MAX_INHERITS {
                break;
            }
            let Some(s) = self.scheme(&name) else { break };
            cur = s.inherits.clone();
            out.push((name, s));
        }
        out
    }

    /// The names of schemes that `scheme` refers to (itself, or through
    /// `inherits`) that aren't defined.
    pub fn missing(&self, scheme: Option<&str>) -> Option<String> {
        let chain = self.chain(scheme);
        match chain.last() {
            None => scheme.map(str::to_owned),
            Some((_, s)) => s.inherits.clone().filter(|_| chain.len() < MAX_INHERITS),
        }
    }

    /// Resolves the styles for the scheme `scheme`.
    pub fn resolver(&self, scheme: Option<&str>) -> Resolver<'_> {
        Resolver {
            styles: self,
            chain: self.chain(scheme),
        }
    }

    /// Every name that has a value somewhere: [`ROLES`], then the others,
    /// sorted.
    pub fn names(&self) -> Vec<String> {
        let mut extra: Vec<&String> = (self.user.keys())
            .chain(self.defaults.keys())
            .chain(self.schemes.values().flat_map(|s| s.values.keys()))
            .filter(|n| !ROLES.contains(&n.as_str()))
            .collect();
        extra.sort();
        extra.dedup();
        ROLES
            .iter()
            .map(|&r| r.to_owned())
            .chain(extra.into_iter().cloned())
            .collect()
    }

    /// The state, as (name, command) pairs, for `style -p` and the
    /// startup cache (`state.rs`). The command is `cmd` and its arguments.
    pub fn state(&self, cmd: &str) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for (name, s) in &self.schemes {
            let parent = s.inherits.as_deref().unwrap_or("");
            let t = format!("{cmd} -s {} -i {}\n", q(name), q(parent));
            out.push((format!("s:{name}"), t.into_bytes()));
            for (role, v) in &s.values {
                let t = format!("{cmd} -s {} {} {}\n", q(name), q(role), q(&v.text()));
                out.push((format!("s:{name}:{role}"), t.into_bytes()));
            }
            for (key, v) in &s.terminal {
                let t = format!("{cmd} -s {} {TERMINAL_PREFIX}{key} {}\n", q(name), q(&terminal_text(v)));
                out.push((format!("s:{name}:{TERMINAL_PREFIX}{key}"), t.into_bytes()));
            }
        }
        for (role, v) in &self.defaults {
            let t = format!("{cmd} -d {} {}\n", q(role), q(&v.text()));
            out.push((format!("d:{role}"), t.into_bytes()));
        }
        if let Some(c) = &self.choice {
            let t = match c {
                Choice::Nothing => format!("{cmd} --clear\n"),
                c => {
                    let names: Vec<String> = c.names().into_iter().map(q).collect();
                    format!("{cmd} -c {}\n", names.join(" "))
                }
            };
            out.push(("choice".to_owned(), t.into_bytes()));
        }
        for (role, v) in &self.user {
            let t = format!("{cmd} {} {}\n", q(role), q(&v.text()));
            out.push((format!("u:{role}"), t.into_bytes()));
        }
        if self.no_terminal_colors {
            out.push((
                "terminal-colors".to_owned(),
                format!("{cmd} --terminal-colors off\n").into_bytes(),
            ));
        }
        out
    }
}

/// A word for a command: quoted unless it is a name (as style and scheme
/// names are).
fn q(s: &str) -> String {
    let plain = s
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'));
    match plain && !s.is_empty() {
        true => s.to_owned(),
        false => String::from_utf8_lossy(&crate::builtins::single_quote(s.as_bytes())).into_owned(),
    }
}

/// The command that removes the piece of state `name` (as [`Styles::state`]
/// names it).
pub fn removal(cmd: &str, name: &str) -> String {
    let (kind, rest) = name.split_once(':').unwrap_or((name, ""));
    match (kind, rest.split_once(':')) {
        ("s", Some((scheme, role))) => format!("{cmd} -s {} -r {}", q(scheme), q(role)),
        ("s", None) => format!("{cmd} -s {} --delete", q(rest)),
        ("d", _) => format!("{cmd} -d -r {}", q(rest)),
        ("u", _) => format!("{cmd} -r {}", q(rest)),
        ("terminal-colors", _) => format!("{cmd} --terminal-colors on"),
        _ => format!("{cmd} -c {} {}", DEFAULT_PAIR.0, DEFAULT_PAIR.1),
    }
}

/// The styles of one scheme, merged with the other layers.
pub struct Resolver<'a> {
    styles: &'a Styles,
    chain: Vec<(String, Scheme)>,
}

impl Resolver<'_> {
    /// The value of `name` itself, from the highest layer that has one.
    pub fn value(&self, name: &str) -> Option<(Style, Origin)> {
        if let Some(v) = self.styles.user.get(name) {
            return Some((v.clone(), Origin::User));
        }
        for (scheme, s) in &self.chain {
            if let Some(v) = s.values.get(name) {
                return Some((v.clone(), Origin::Scheme(scheme.clone())));
            }
        }
        (self.styles.defaults.get(name)).map(|v| (v.clone(), Origin::Plugin))
    }

    /// The style of `name`, with what it leaves out from its parents.
    pub fn get(&self, name: &str) -> Style {
        let mut style = self.value(name).map(|v| v.0).unwrap_or_default();
        let mut cur = parent(name);
        while let Some(p) = cur {
            if style.plain {
                break;
            }
            if let Some((v, _)) = self.value(p) {
                style.inherit(&v);
            }
            cur = parent(p);
        }
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Style, String> {
        Style::parse(&[s])
    }

    #[test]
    fn colors() {
        assert_eq!(Color::parse(b"red"), Some(Color::Index(1)));
        assert_eq!(Color::parse(b"bright-white"), Some(Color::Index(15)));
        assert_eq!(Color::parse(b"208"), Some(Color::Index(208)));
        assert_eq!(Color::parse(b"256"), None);
        assert_eq!(Color::parse(b"#f08"), Some(Color::Rgb(255, 0, 136)));
        assert_eq!(Color::parse(b"default"), Some(Color::Default));
        assert_eq!(Color::parse(b"bright-"), None);
        assert_eq!(Color::parse(b""), None);
    }

    #[test]
    fn values() {
        let s = parse("bold  blue bg:#102030 no-underline").unwrap();
        assert_eq!(s.text(), "bold no-underline blue bg:#102030");
        assert_eq!(s.sgr(), "1;34;48;2;16;32;48");
        assert_eq!(Style::parse(&["bright-red", "italic"]).unwrap().sgr(), "3;91");
        assert_eq!(parse("sgr:1;38;5;208").unwrap().sgr(), "1;38;5;208");
        assert_eq!(parse("").unwrap().text(), "plain");
        assert_eq!(parse("plain").unwrap().text(), "plain");
        assert_eq!(parse("blue red"), Err("two colours: red".into()));
        assert_eq!(parse("bolt"), Err("bad style: bolt".into()));
        assert_eq!(parse("bg:nope"), Err("bad colour: bg:nope".into()));
        assert_eq!(parse("sgr:1m"), Err("bad SGR parameters: sgr:1m".into()));
    }

    #[test]
    fn underlines() {
        let s = parse("red undercurl ul:#ff0000").unwrap();
        assert_eq!(s.text(), "undercurl red ul:#ff0000");
        assert_eq!(s.sgr(), "4:3;31;58;2;255;0;0");
        assert_eq!(parse("underdashed ul:blue").unwrap().sgr(), "4:5;58;5;4");
        assert_eq!(parse("underline ul:default").unwrap().sgr(), "4;59");
        assert_eq!(parse("ul:red ul:blue"), Err("two underline colours: ul:blue".into()));
        assert_eq!(parse("ul:x"), Err("bad colour: ul:x".into()));
        // A child takes the parent's kind of underline with its underline,
        // not when it underlines itself, nor when it turns it off.
        let parent = parse("underdotted ul:red").unwrap();
        assert_eq!(parent.add(&parse("bold").unwrap()).sgr(), "1;4:4;58;5;1");
        assert_eq!(parent.add(&parse("underline").unwrap()).sgr(), "4;58;5;1");
        assert_eq!(parent.add(&parse("no-underline").unwrap()).sgr(), "58;5;1");
        assert_eq!(parent.add(&parse("ul:green").unwrap()).text(), "underdotted ul:green");
    }

    #[test]
    fn names() {
        assert_eq!(check_name("command.unknown"), Ok(()));
        assert_eq!(check_name("git.branch"), Ok(()));
        assert_eq!(check_name("prompt.dir"), Ok(()));
        assert_eq!(
            check_name("command.unknwn"),
            Err("unknown style: command.unknwn; did you mean command.unknown?".into())
        );
        assert_eq!(
            check_name("command.nosuchthing"),
            Err("unknown style: command.nosuchthing".into())
        );
        assert_eq!(
            check_name("comand"),
            Err("unknown style: comand; did you mean command?".into())
        );
        assert_eq!(check_name("colorscheme"), Err("bad style name: colorscheme".into()));
        assert_eq!(check_name("a..b"), Err("bad style name: a..b".into()));
        assert_eq!(check_name("a b"), Err("bad style name: a b".into()));
    }

    #[test]
    fn backgrounds() {
        assert_eq!(background(Some(b"light"), Some(b"15;0")), Some(Background::Light));
        assert_eq!(background(Some(b"bogus"), Some(b"15;0")), Some(Background::Dark));
        assert_eq!(background(None, Some(b"0;default;15")), Some(Background::Light));
        assert_eq!(background(None, Some(b"0;default")), None);
        assert_eq!(background(None, None), None);
    }

    #[test]
    fn layers_and_fallback() {
        let mut st = Styles::default();
        let r = st.resolver(Some("default-dark"));
        assert_eq!(r.get("command.builtin").text(), "green");
        assert_eq!(r.get("command.alias").text(), "italic green");
        assert_eq!(r.get("command.unknown").text(), "bold red");
        assert_eq!(r.get("var.unset").text(), "dim cyan");
        let light = st.resolver(Some("default-light"));
        assert_eq!(light.get("string.single").text(), "136");
        assert_eq!(light.get("keyword").text(), "bold blue");

        st.set_in_scheme("blue", "command", parse("blue").unwrap());
        st.set_in_scheme("blue", "command.unknown", parse("bold red").unwrap());
        st.set_in_scheme("green", "keyword", parse("bold green").unwrap());
        st.set_inherits("green", Some("blue")).unwrap();
        assert!(st.set_inherits("blue", Some("green")).is_err());
        let r = st.resolver(Some("green"));
        assert_eq!(r.get("command.alias").text(), "blue");
        // Schemes start empty.
        assert_eq!(r.get("string").text(), "plain");
        // The user's `command` doesn't hide the scheme's `command.unknown`,
        // and a value replaces the scheme's as a whole.
        st.set_user("command", parse("italic").unwrap());
        st.set_user("keyword", parse("underline").unwrap());
        let r = st.resolver(Some("green"));
        assert_eq!(r.get("command.unknown").text(), "bold italic red");
        assert_eq!(r.get("command.alias").text(), "italic");
        assert_eq!(r.get("keyword").text(), "underline");
        assert_eq!(r.value("keyword").map(|v| v.1), Some(Origin::User));
        // `plain` stops the fallback; `no-` takes an attribute away.
        st.set_user("command.alias", parse("plain").unwrap());
        st.set_user("command.function", parse("no-italic red").unwrap());
        let r = st.resolver(Some("green"));
        assert_eq!(r.get("command.alias").sgr(), "");
        assert_eq!(r.get("command.function").sgr(), "31");
        // Plugins' defaults are under everything.
        st.set_default("git.branch", parse("magenta").unwrap());
        st.set_default("command", parse("red").unwrap());
        let r = st.resolver(Some("green"));
        assert_eq!(r.get("git.branch.dirty").text(), "magenta");
        assert_eq!(r.get("command").text(), "italic");
        assert_eq!(st.missing(Some("green")), None);
        assert_eq!(st.missing(Some("nope")), Some("nope".into()));
        st.set_inherits("blue", Some("later")).unwrap();
        assert_eq!(st.missing(Some("green")), Some("later".into()));
    }

    #[test]
    fn choices() {
        let mut st = Styles::default();
        assert_eq!(st.choice().scheme(None), Some("default-dark"));
        assert_eq!(st.choice().scheme(Some(Background::Light)), Some("default-light"));
        let pair = Choice::Pair {
            dark: "a".into(),
            light: "b".into(),
            default: Some("c".into()),
        };
        st.set_choice(pair.clone());
        assert_eq!(st.choice().scheme(None), Some("c"));
        assert_eq!(st.choice().names(), ["a", "b", "c"]);
        // The built-in pair is no change.
        st.set_choice(Choice::Pair {
            dark: "default-dark".into(),
            light: "default-light".into(),
            default: None,
        });
        assert!(st.state("style").is_empty());
        st.clear();
        assert_eq!(st.choice().scheme(None), None);
    }

    #[test]
    fn state() {
        let mut st = Styles::default();
        st.set_in_scheme("mine", "keyword", Style::parse(&["bold"]).unwrap());
        st.set_user("var", Style::parse(&["cyan"]).unwrap());
        st.set_choice(Choice::One("mine".into()));
        let s: Vec<_> = st
            .state("style")
            .into_iter()
            .map(|(n, t)| format!("{n} {}", String::from_utf8(t).unwrap()))
            .collect();
        assert_eq!(
            s,
            [
                "s:mine style -s mine -i ''\n",
                "s:mine:keyword style -s mine keyword bold\n",
                "choice style -c mine\n",
                "u:var style var cyan\n",
            ]
        );
        assert_eq!(removal("style", "s:t:keyword"), "style -s t -r keyword");
        assert_eq!(removal("style", "s:t"), "style -s t --delete");
        assert_eq!(removal("style", "u:var"), "style -r var");
        assert_eq!(removal("style", "d:x"), "style -d -r x");
        assert_eq!(removal("style", "choice"), "style -c default-dark default-light");
    }

    #[test]
    fn terminal_colors() {
        assert_eq!(parse_terminal("background", &["#282828"]), Ok(vec![(0x28, 0x28, 0x28)]));
        assert_eq!(
            parse_terminal("palette", &["#000 #ff0000", "#00ff00"]),
            Ok(vec![(0, 0, 0), (255, 0, 0), (0, 255, 0)])
        );
        assert!(parse_terminal("palette", &["#000000"; 17]).is_err());
        assert!(parse_terminal("cursor", &["#000", "#fff"]).is_err());
        assert!(parse_terminal("cursor", &["red"]).is_err());
        assert!(parse_terminal("cursor", &[""]).is_err());
        assert!(parse_terminal("bold", &["#000"]).is_err());
        assert_eq!(terminal_text(&[(0x28, 0x28, 0x28), (255, 0, 0)]), "#282828 #ff0000");
        // Each key from the first scheme of the chain that gives it.
        let mut st = Styles::default();
        st.set_terminal_in_scheme("dark", "background", vec![(0, 0, 0)]);
        st.set_terminal_in_scheme("dark", "foreground", vec![(255, 255, 255)]);
        st.set_inherits("light", Some("dark")).unwrap();
        st.set_terminal_in_scheme("light", "background", vec![(255, 255, 255)]);
        let t = st.terminal(Some("light"));
        assert_eq!(t["background"], [(255, 255, 255)]);
        assert_eq!(t["foreground"], [(255, 255, 255)]);
        assert!(st.terminal(Some("default-dark")).is_empty());
        st.remove_from_scheme("light", "terminal.background");
        assert_eq!(st.terminal(Some("light"))["background"], [(0, 0, 0)]);
        // The saved state, and undoing it.
        st.set_terminal_colors(false);
        let s: Vec<_> = st
            .state("style")
            .into_iter()
            .map(|(n, t)| format!("{n} {}", String::from_utf8(t).unwrap()))
            .collect();
        assert_eq!(
            s,
            [
                "s:dark style -s dark -i ''\n",
                "s:dark:terminal.background style -s dark terminal.background '#000000'\n",
                "s:dark:terminal.foreground style -s dark terminal.foreground '#ffffff'\n",
                "s:light style -s light -i dark\n",
                "terminal-colors style --terminal-colors off\n",
            ]
        );
        assert_eq!(removal("style", "terminal-colors"), "style --terminal-colors on");
        assert_eq!(
            removal("style", "s:dark:terminal.background"),
            "style -s dark -r terminal.background"
        );
        // Not styles.
        assert!(check_name("terminal.background").is_err());
        assert!(check_name("terminal").is_err());
        assert!(check_name("terminal-colours").is_err());
        assert!(check_name("terminal_x").is_ok());
    }
}
