//! The completion menu, like zsh's menu selection.
//!
//! When Tab can't complete any further, the completer opens the menu: the
//! matches are drawn below the line as rustyline's hint, so that rustyline
//! lays them out with the line and erases them when the line changes or is
//! accepted. The next Tab selects the first match and puts it in the line in
//! place of the word; more Tabs, Shift-Tab and the arrow keys move the
//! selection. The menu is open only while the line and the cursor are the
//! ones it left, so typing anything keeps the match and closes the menu.
//!
//! The key bindings (`bind`) only record a move and ask rustyline to
//! complete (`Cmd::Complete`). The completer makes the move and gives
//! rustyline the new text as the only candidate (`Menu::step`), so every
//! change to the line goes through rustyline's completion and its undo.

use std::sync::{Arc, Mutex};

use rustyline::hint::Hint;
use rustyline::history::History;
use rustyline::{
    Cmd, ConditionalEventHandler, Editor, Event, EventContext, EventHandler, Helper, KeyCode, KeyEvent, Modifiers,
    RepeatCount,
};
use unicode_width::UnicodeWidthChar;

/// A match, as the menu shows it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Item {
    pub display: String,
    pub desc: Option<String>,
    /// The text that replaces the word in the line.
    pub replacement: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    Next,
    Prev,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    /// Puts back the text typed and closes the menu.
    Cancel,
}

/// How the items are laid out. Items with descriptions go one per row;
/// others go in a grid with as many columns as fit, each as wide as its
/// widest item. So that a few long items don't leave one column, they are
/// cut to the width of most items (or a third of the screen, if more). The
/// grid goes down the columns (as `ls` lists files) if all
/// the rows fit on the screen, otherwise along the rows, so that the rows
/// shown hold consecutive items as they scroll.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Grid {
    rows: usize,
    cols: usize,
    by_column: bool,
    /// The width of each column, without the two spaces between columns,
    /// or the width names are padded to before their descriptions.
    widths: Vec<usize>,
    /// How many rows are shown at a time.
    shown: usize,
    /// Whether the items have descriptions (and so go one per row).
    descs: bool,
}

impl Grid {
    /// The layout of `items` in a terminal `cols` wide, with `avail` rows
    /// below the line.
    fn new(items: &[Item], cols: usize, avail: usize) -> Grid {
        let n = items.len();
        let usable = cols.saturating_sub(1).max(1);
        let mut w: Vec<usize> = items.iter().map(|i| width(&i.display).clamp(1, usable)).collect();
        let descs = items.iter().any(|i| i.desc.is_some());
        if !descs && n > 0 {
            let mut sorted = w.clone();
            let most = *sorted.select_nth_unstable(n * 9 / 10).1;
            let cap = most.max(usable / 3);
            w.iter_mut().for_each(|w| *w = (*w).min(cap));
        }
        let (rows, cols, by_column, widths) = if descs {
            let widest = w.iter().copied().max().unwrap_or(0);
            (n, 1, true, vec![widest.min(usable / 2)])
        } else {
            // The widths of the columns of a grid with `cols` columns, if
            // it fits.
            let fit = |cols: usize, by_column: bool| {
                let rows = n.div_ceil(cols);
                let cols = if by_column { n.div_ceil(rows) } else { cols };
                let mut widths = vec![0; cols];
                for (i, &w) in w.iter().enumerate() {
                    let c = if by_column { i / rows } else { i % cols };
                    widths[c] = widths[c].max(w);
                }
                let total = widths.iter().sum::<usize>() + 2 * (cols - 1);
                (total <= usable).then_some((rows, cols, by_column, widths))
            };
            let most = n.min(usable.div_ceil(3)).max(1);
            let fits = |by_column| (1..=most).rev().find_map(|c| fit(c, by_column));
            match fits(true) {
                Some(g) if g.0 <= avail => g,
                _ => fits(false).unwrap_or((n, 1, false, vec![usable])),
            }
        };
        // When the rows don't all fit, the last one says which are shown.
        let shown = if rows <= avail || avail < 2 {
            rows.min(avail)
        } else {
            avail - 1
        };
        Grid {
            rows,
            cols,
            by_column,
            widths,
            shown,
            descs,
        }
    }

    fn index(&self, row: usize, col: usize) -> usize {
        if self.by_column {
            col * self.rows + row
        } else {
            row * self.cols + col
        }
    }

    /// The row and column of item `i`.
    fn cell(&self, i: usize) -> (usize, usize) {
        if self.by_column {
            (i % self.rows, i / self.rows)
        } else {
            (i / self.cols, i % self.cols)
        }
    }
}

/// The SGR parameters for the menu (each may be empty).
pub struct Style {
    pub select: String,
    pub desc: String,
}

#[derive(Default)]
pub struct Menu {
    open: bool,
    /// The line and the cursor position the menu is open for.
    line: String,
    pos: usize,
    /// Where the word being completed starts, and the text typed.
    start: usize,
    typed: String,
    items: Vec<Item>,
    selected: Option<usize>,
    /// A move asked for by a key binding, which the next completion makes.
    pub(super) pending: Option<Move>,
    /// The layout the menu was last drawn with, for moving in it.
    grid: Grid,
    /// The first row shown.
    top: usize,
}

impl Menu {
    /// Opens the menu with `items`, which complete the text of `line` from
    /// `start` to the cursor at `pos`.
    pub fn open(&mut self, line: &str, start: usize, pos: usize, items: Vec<Item>) {
        *self = Menu {
            open: true,
            line: line.to_owned(),
            pos,
            start,
            typed: line.get(start..pos).unwrap_or_default().to_owned(),
            items,
            ..Menu::default()
        };
    }

    pub fn close(&mut self) {
        *self = Menu::default();
    }

    pub fn is_open(&self, line: &str, pos: usize) -> bool {
        self.open && self.pos == pos && self.line == line
    }

    /// If the menu is open for `line` and `pos`, makes the pending move (a
    /// Tab, if none), and returns where the word starts and the text that
    /// replaces it up to the cursor. Otherwise closes the menu.
    pub fn step(&mut self, line: &str, pos: usize) -> Option<(usize, String)> {
        if !self.is_open(line, pos) {
            self.close();
            return None;
        }
        let text = match self.pending.take().unwrap_or(Move::Next) {
            Move::Cancel => {
                self.open = false;
                self.typed.clone()
            }
            m => {
                let i = self.moved(m);
                self.selected = Some(i);
                self.items[i].replacement.clone()
            }
        };
        self.line = [&line[..self.start], &text, &line[pos..]].concat();
        self.pos = self.start + text.len();
        Some((self.start, text))
    }

    /// The item that move `m` selects.
    fn moved(&self, m: Move) -> usize {
        let n = self.items.len();
        let Some(i) = self.selected else {
            return match m {
                Move::Prev | Move::Up | Move::Left | Move::PageUp => n - 1,
                _ => 0,
            };
        };
        let g = if self.grid.rows == 0 {
            &Grid::new(&self.items, usize::MAX, usize::MAX)
        } else {
            &self.grid
        };
        let (r, c) = g.cell(i);
        let last_row = (0..g.rows).rev().find(|&r| g.index(r, c) < n).unwrap_or(0);
        let last_col = (0..g.cols).rev().find(|&c| g.index(r, c) < n).unwrap_or(0);
        let page = g.shown.max(1);
        match m {
            Move::Next => (i + 1) % n,
            Move::Prev => (i + n - 1) % n,
            Move::Right if g.cols == 1 => (i + 1) % n,
            Move::Left if g.cols == 1 => (i + n - 1) % n,
            Move::Down if r < last_row => g.index(r + 1, c),
            Move::Down => g.index(0, c),
            Move::Up if r > 0 => g.index(r - 1, c),
            Move::Up => g.index(last_row, c),
            Move::Right if c < last_col => g.index(r, c + 1),
            Move::Right => g.index(r, 0),
            Move::Left if c > 0 => g.index(r, c - 1),
            Move::Left => g.index(r, last_col),
            Move::PageDown if r < last_row => g.index((r + page).min(last_row), c),
            Move::PageDown => g.index(0, c),
            Move::PageUp if r > 0 => g.index(r.saturating_sub(page), c),
            Move::PageUp => g.index(last_row, c),
            Move::Cancel => i,
        }
    }

    /// Draws the menu for a terminal `cols` wide, with `avail` rows below
    /// the line. Each row starts with a newline.
    pub fn draw(&mut self, cols: usize, avail: usize, style: &Style) -> String {
        self.grid = Grid::new(&self.items, cols, avail);
        let g = &self.grid;
        if let Some(i) = self.selected {
            let r = g.cell(i).0;
            if r < self.top {
                self.top = r;
            } else if r >= self.top + g.shown {
                self.top = r + 1 - g.shown;
            }
        }
        self.top = self.top.min(g.rows - g.shown);
        let usable = cols.saturating_sub(1).max(1);
        let mut out = String::new();
        for r in self.top..self.top + g.shown {
            out.push('\n');
            for c in 0..g.cols {
                let i = g.index(r, c);
                let Some(item) = self.items.get(i) else { break };
                let selected = self.selected == Some(i);
                if c > 0 {
                    out.push_str("  ");
                }
                if g.descs {
                    let (name, w) = truncate(&item.display, usable);
                    let pad = g.widths[0].saturating_sub(w);
                    let l = (pad + 2).min(usable - w);
                    let lead = " ".repeat(l);
                    let desc = (item.desc.as_ref()).map(|d| truncate(&format!("-- {d}"), usable - w - l).0);
                    match desc {
                        Some(desc) if selected => styled(&mut out, &style.select, &format!("{name}{lead}{desc}")),
                        Some(desc) => {
                            out.push_str(&name);
                            out.push_str(&lead);
                            styled(&mut out, &style.desc, &desc);
                        }
                        None if selected => styled(&mut out, &style.select, &format!("{name}{:pad$}", "")),
                        None => out.push_str(&name),
                    }
                    continue;
                }
                let (name, w) = truncate(&item.display, g.widths[c]);
                let last = c + 1 == g.cols || self.items.get(g.index(r, c + 1)).is_none();
                if selected || !last {
                    let cell = format!("{name}{:1$}", "", g.widths[c] - w);
                    styled(&mut out, if selected { &style.select } else { "" }, &cell);
                } else {
                    out.push_str(&name);
                }
            }
        }
        if g.shown < g.rows && g.shown > 0 {
            let at = format!("rows {}-{} of {}", self.top + 1, self.top + g.shown, g.rows);
            out.push('\n');
            styled(&mut out, &style.desc, &truncate(&at, usable).0);
        }
        out
    }
}

/// Appends `text`, in the style given by the SGR parameters `sgr`.
fn styled(out: &mut String, sgr: &str, text: &str) {
    if sgr.is_empty() || text.is_empty() {
        out.push_str(text);
    } else {
        out.push_str(&format!("\x1b[{sgr}m{text}\x1b[0m"));
    }
}

/// How many columns `s` takes on the terminal.
fn width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// `s` cut to at most `max` columns, ending with `…` if it was cut, and the
/// columns it takes.
fn truncate(s: &str, max: usize) -> (String, usize) {
    if width(s) <= max {
        return (s.to_owned(), width(s));
    }
    let mut w = 0;
    let mut out = String::new();
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        w += cw;
        out.push(c);
    }
    if max > 0 {
        out.push('…');
        w += 1;
    }
    (out, w)
}

/// How many rows `text` takes on a terminal `cols` wide, counted as
/// rustyline does, without escape sequences. A text that ends at the right
/// margin takes another row, where the cursor goes.
pub fn rows(text: &str, cols: usize) -> usize {
    let cols = if cols == 0 { 80 } else { cols };
    let (mut row, mut col) = (0, 0);
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\n' => (row, col) = (row + 1, 0),
            '\x1b' => {
                if chars.next() == Some('[') {
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
            }
            c => {
                let w = c.width().unwrap_or(0);
                col += w;
                if col > cols {
                    (row, col) = (row + 1, w);
                }
            }
        }
    }
    row + 1 + usize::from(col == cols)
}

/// The menu, as rustyline's hint.
pub struct Drawn(pub String);

impl Hint for Drawn {
    fn display(&self) -> &str {
        &self.0
    }

    /// Nothing: moving right at the end of the line doesn't insert the
    /// menu.
    fn completion(&self) -> Option<&str> {
        None
    }
}

#[derive(Clone, Copy)]
enum Action {
    Move(Move),
    /// Keeps the selected match and closes the menu.
    Accept,
}

/// A key that acts on the menu while it is open (and otherwise does what
/// it normally does). The arrow keys, other than Down, move only once a
/// match is selected, so they go on moving in the line and the history
/// while the menu only lists the matches.
struct Key {
    menu: Arc<Mutex<Menu>>,
    action: Action,
}

impl ConditionalEventHandler for Key {
    fn handle(&self, _: &Event, _: RepeatCount, _: bool, ctx: &EventContext) -> Option<Cmd> {
        let mut menu = self.menu.lock().ok()?;
        if !menu.is_open(ctx.line(), ctx.pos()) {
            return None;
        }
        let selecting = menu.selected.is_some();
        match self.action {
            Action::Accept if selecting => {
                menu.close();
                Some(Cmd::Repaint)
            }
            Action::Move(Move::Cancel) if !selecting => {
                menu.close();
                Some(Cmd::Repaint)
            }
            Action::Move(m) if selecting || matches!(m, Move::Cancel | Move::Down | Move::Prev) => {
                menu.pending = Some(m);
                Some(Cmd::Complete)
            }
            _ => None,
        }
    }
}

/// Binds the keys that act on the menu.
pub fn bind<H: Helper, I: History>(ed: &mut Editor<H, I>, menu: &Arc<Mutex<Menu>>) {
    use KeyCode as K;
    let none = Modifiers::NONE;
    let keys = [
        (KeyEvent(K::BackTab, none), Action::Move(Move::Prev)),
        (KeyEvent(K::Down, none), Action::Move(Move::Down)),
        (KeyEvent(K::Up, none), Action::Move(Move::Up)),
        (KeyEvent(K::Right, none), Action::Move(Move::Right)),
        (KeyEvent(K::Left, none), Action::Move(Move::Left)),
        (KeyEvent::ctrl('N'), Action::Move(Move::Down)),
        (KeyEvent::ctrl('P'), Action::Move(Move::Up)),
        (KeyEvent::ctrl('F'), Action::Move(Move::Right)),
        (KeyEvent::ctrl('B'), Action::Move(Move::Left)),
        (KeyEvent(K::PageDown, none), Action::Move(Move::PageDown)),
        (KeyEvent(K::PageUp, none), Action::Move(Move::PageUp)),
        (KeyEvent(K::Esc, none), Action::Move(Move::Cancel)),
        (KeyEvent::ctrl('G'), Action::Move(Move::Cancel)),
        (KeyEvent(K::Enter, none), Action::Accept),
    ];
    for (key, action) in keys {
        let menu = Arc::clone(menu);
        ed.bind_sequence(key, EventHandler::Conditional(Box::new(Key { menu, action })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(names: &[&str]) -> Vec<Item> {
        (names.iter())
            .map(|n| Item {
                display: n.to_string(),
                desc: None,
                replacement: format!("{n} "),
            })
            .collect()
    }

    fn plain() -> Style {
        Style {
            select: "7".into(),
            desc: String::new(),
        }
    }

    #[test]
    fn grid() {
        let names: Vec<String> = (0..10).map(|i| format!("item{i}")).collect();
        let names: Vec<&str> = names.iter().map(|s| &s[..]).collect();
        // Four columns of 5 fit in 30 columns; 10 items take 3 rows, and
        // then 4 columns.
        let g = Grid::new(&items(&names), 30, 10);
        assert_eq!((g.rows, g.cols, g.by_column, g.shown), (3, 4, true, 3));
        assert_eq!(g.widths, [5, 5, 5, 5]);
        assert_eq!((g.index(2, 1), g.cell(5)), (5, (2, 1)));
        // Too many rows for the screen: along the rows, and scrolling.
        let g = Grid::new(&items(&names), 30, 2);
        assert_eq!((g.rows, g.cols, g.by_column, g.shown), (3, 4, false, 1));
        assert_eq!((g.index(2, 1), g.cell(5)), (9, (1, 1)));
        // Each column as wide as its widest item.
        let g = Grid::new(&items(&["a", "b", "long name", "c"]), 16, 10);
        assert_eq!((g.rows, g.cols, g.widths), (2, 2, vec![1, 9]));
        // Descriptions: one per row.
        let mut it = items(&["a", "bb"]);
        it[0].desc = Some("x".into());
        let g = Grid::new(&it, 80, 10);
        assert_eq!((g.rows, g.cols, g.widths), (2, 1, vec![2]));
    }

    #[test]
    fn moves() {
        let mut m = Menu::default();
        m.open("ls x", 3, 4, items(&["a", "b", "c", "d", "e"]));
        // Column by column: a d / b e / c.
        m.draw(5, 10, &plain());
        let mut go = |mv| {
            m.pending = Some(mv);
            m.step(&m.line.clone(), m.pos).unwrap().1
        };
        assert_eq!(go(Move::Next), "a ");
        assert_eq!(go(Move::Right), "d ");
        assert_eq!(go(Move::Down), "e ");
        assert_eq!(go(Move::Down), "d ");
        assert_eq!(go(Move::Up), "e ");
        assert_eq!(go(Move::Right), "b ");
        assert_eq!(go(Move::Left), "e ");
        assert_eq!(go(Move::Next), "a ");
        assert_eq!(go(Move::Prev), "e ");
        assert_eq!(go(Move::Up), "d ");
        assert_eq!(go(Move::Left), "a ");
        assert_eq!(go(Move::Up), "c ");
        assert_eq!(go(Move::Right), "c ");
        assert_eq!(go(Move::Cancel), "x");
        assert!(!m.open);
        // Before anything is selected.
        m.open("ls x", 3, 4, items(&["a", "b", "c"]));
        m.pending = Some(Move::Prev);
        assert_eq!(m.step("ls x", 4), Some((3, "c ".into())));
        assert_eq!(m.line, "ls c ");
        // The line changed: the menu is closed.
        assert_eq!(m.step("ls c x", 6), None);
        assert!(!m.open);
    }

    #[test]
    fn draw() {
        let mut m = Menu::default();
        m.open("ls ", 3, 3, items(&["alpha", "beta", "gamma", "delta", "epsilon"]));
        let style = plain();
        assert_eq!(m.draw(30, 10, &style), "\nalpha  gamma  epsilon\nbeta   delta");
        m.pending = Some(Move::Next);
        m.step("ls ", 3);
        m.pending = Some(Move::Right);
        m.step("ls alpha ", 9);
        assert_eq!(
            m.draw(30, 10, &style),
            "\nalpha  \x1b[7mgamma\x1b[0m  epsilon\nbeta   delta"
        );
        // Too few rows: along the rows, scrolled to the selection.
        assert_eq!(m.draw(18, 2, &style), "\n\x1b[7mgamma  \x1b[0m  delta\nrows 2-2 of 3");
        m.selected = Some(4);
        assert_eq!(m.draw(18, 2, &style), "\n\x1b[7mepsilon\x1b[0m\nrows 3-3 of 3");
        assert_eq!(m.draw(18, 0, &style), "");
        // Descriptions, aligned and cut to the width.
        let mut it = items(&["add", "commit", "x"]);
        it[0].desc = Some("Add files".into());
        it[1].desc = Some("Record changes to the repository".into());
        m.open("git ", 4, 4, it);
        let style = Style {
            select: "7".into(),
            desc: "90".into(),
        };
        assert_eq!(
            m.draw(30, 10, &style),
            "\nadd     \x1b[90m-- Add files\x1b[0m\ncommit  \x1b[90m-- Record changes to…\x1b[0m\nx"
        );
        m.selected = Some(0);
        assert_eq!(
            m.draw(30, 10, &style),
            "\n\x1b[7madd     -- Add files\x1b[0m\ncommit  \x1b[90m-- Record changes to…\x1b[0m\nx"
        );
    }

    #[test]
    fn row_count() {
        assert_eq!(rows("$ ls", 80), 1);
        assert_eq!(rows("\x1b[1;32m$\x1b[0m ls", 5), 1);
        assert_eq!(rows("$ ls", 4), 2);
        assert_eq!(rows("$ ls x", 4), 2);
        assert_eq!(rows("a\nb", 80), 2);
        assert_eq!(rows("日本語", 4), 2);
    }
}
