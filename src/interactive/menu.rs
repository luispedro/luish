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
//! While the menu only lists the matches (none is selected), it follows the
//! word as it is typed (`Menu::follow`, which the hinter calls): the matches
//! that go on matching are the ones listed, and deleting brings the others
//! back, as far as the text the menu was opened for.
//!
//! The keys (`key`, called by the key bindings of `keys.rs`) only record
//! a move and ask rustyline to complete (`Cmd::Complete`). The completer
//! makes the move and gives
//! rustyline the new text as the only candidate (`Menu::step`), so every
//! change to the line goes through rustyline's completion and its undo.

use std::sync::Mutex;

use rustyline::hint::Hint;
use rustyline::{Cmd, EventContext, KeyCode, KeyEvent, Modifiers};
use unicode_width::UnicodeWidthChar;

/// What a match is, for its colour in the menu.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Kind {
    #[default]
    Plain,
    Dir,
    /// An executable file.
    Exec,
}

/// A match, as the menu shows it.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Item {
    pub display: String,
    pub desc: Option<String>,
    /// The text that replaces the word in the line.
    pub replacement: String,
    pub kind: Kind,
    /// Its group: the position of its heading in [`Matches::groups`].
    pub group: u16,
}

/// The matches for a word, those of a group next to each other, and the
/// headings of the groups (empty for the matches that are in none). The
/// headings are shown when the matches listed are of more than one group.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Matches {
    pub items: Vec<Item>,
    pub groups: Vec<String>,
}

impl From<Vec<Item>> for Matches {
    /// Matches that are in no group.
    fn from(items: Vec<Item>) -> Matches {
        Matches {
            items,
            groups: Vec::new(),
        }
    }
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

/// How the items of a group are laid out. Items with descriptions go one
/// per row; others go in a grid with as many columns as fit, each as wide
/// as its widest item. So that a few long items don't leave one column (or
/// push the descriptions to the right edge), names are cut to the width of
/// most of them (or a third of the screen, if more); before a description
/// a longer name is shown whole instead, and only its own description
/// starts further right.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Grid {
    rows: usize,
    cols: usize,
    by_column: bool,
    /// The width of each column, without the two spaces between columns,
    /// or the width names are padded to before their descriptions.
    widths: Vec<usize>,
    /// Whether the items have descriptions (and so go one per row).
    descs: bool,
}

impl Grid {
    /// The layout of items whose names take `w` columns each, where
    /// `usable` columns can be written to. A grid goes down its columns (as
    /// `ls` lists files) if `by_column`, and otherwise along its rows.
    fn new(w: &[usize], descs: bool, usable: usize, by_column: bool) -> Grid {
        let n = w.len();
        let mut w: Vec<usize> = w.iter().map(|w| (*w).clamp(1, usable)).collect();
        let cap = if n == 0 {
            usable
        } else {
            let mut sorted = w.clone();
            (*sorted.select_nth_unstable(n * 9 / 10).1).max(usable / 3)
        };
        if descs {
            // (The widest of the names that aren't longer than that.)
            let cap = cap.min(usable / 2);
            let widest = w.iter().copied().filter(|&w| w <= cap).max().unwrap_or(cap);
            return Grid {
                rows: n,
                cols: 1,
                by_column: true,
                widths: vec![widest],
                descs,
            };
        }
        w.iter_mut().for_each(|w| *w = (*w).min(cap));
        // The grid with `cols` columns, if it fits.
        let fit = |cols: usize| {
            let rows = n.div_ceil(cols);
            let cols = if by_column { n.div_ceil(rows) } else { cols };
            let mut widths = vec![0; cols];
            for (i, &w) in w.iter().enumerate() {
                let c = if by_column { i / rows } else { i % cols };
                widths[c] = widths[c].max(w);
            }
            let total = widths.iter().sum::<usize>() + 2 * (cols - 1);
            (total <= usable).then_some(Grid {
                rows,
                cols,
                by_column,
                widths,
                descs,
            })
        };
        let most = n.min(usable.div_ceil(3)).max(1);
        (1..=most).rev().find_map(fit).unwrap_or(Grid {
            rows: n,
            cols: 1,
            by_column,
            widths: vec![usable],
            descs,
        })
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

/// The items of a group, as they are laid out.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Section {
    /// Shown in a row of its own, above the items.
    heading: Option<String>,
    /// Its items: the first one (as the items are listed) and how many.
    first: usize,
    len: usize,
    /// The row of its first items, counting from the top of the menu.
    row: usize,
    grid: Grid,
}

/// How the menu is laid out: a section for each group, one below the
/// other, each with its heading when there are several. The grids go down
/// their columns if all the rows fit on the screen, otherwise along their
/// rows, so that the rows shown hold consecutive items as they scroll.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Layout {
    sections: Vec<Section>,
    /// All the rows, with the headings.
    rows: usize,
    /// How many rows are shown at a time.
    shown: usize,
    /// How many items it is the layout of.
    len: usize,
}

impl Layout {
    /// The layout of `items` in a terminal `cols` wide, with `avail` rows
    /// below the line. `groups` are the headings (see [`Matches`]).
    fn new(items: &[&Item], groups: &[String], cols: usize, avail: usize) -> Layout {
        let usable = cols.saturating_sub(1).max(1);
        // The runs of items of the same group: where each starts, and how
        // many items it has.
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            match runs.last_mut() {
                Some((first, len)) if items[*first].group == item.group => *len += 1,
                _ => runs.push((i, 1)),
            }
        }
        let headed = runs.len() > 1;
        let build = |by_column: bool| {
            let mut row = 0;
            let sections: Vec<Section> = (runs.iter())
                .map(|&(first, len)| {
                    let run = &items[first..first + len];
                    let heading = (groups.get(run[0].group as usize))
                        .filter(|h| headed && !h.is_empty())
                        .cloned();
                    let w: Vec<usize> = run.iter().map(|i| width(&i.display)).collect();
                    let descs = run.iter().any(|i| i.desc.is_some());
                    let grid = Grid::new(&w, descs, usable, by_column);
                    row += usize::from(heading.is_some()) + grid.rows;
                    Section {
                        heading,
                        first,
                        len,
                        row: row - grid.rows,
                        grid,
                    }
                })
                .collect();
            (sections, row)
        };
        let (sections, rows) = match build(true) {
            l if l.1 <= avail => l,
            _ => build(false),
        };
        // When the rows don't all fit, the last one says which are shown.
        let shown = if rows <= avail || avail < 2 {
            rows.min(avail)
        } else {
            avail - 1
        };
        Layout {
            sections,
            rows,
            shown,
            len: items.len(),
        }
    }

    /// The section, row and column of item `i`.
    fn cell(&self, i: usize) -> (usize, usize, usize) {
        let s = self.sections.iter().rposition(|s| s.first <= i).unwrap_or(0);
        let (r, c) = self.sections[s].grid.cell(i - self.sections[s].first);
        (s, r, c)
    }

    /// The item at row `r` and column `c` of section `s`, if there is one.
    fn at(&self, s: usize, r: usize, c: usize) -> Option<usize> {
        let sec = &self.sections[s];
        let k = sec.grid.index(r, c);
        (r < sec.grid.rows && c < sec.grid.cols && k < sec.len).then_some(sec.first + k)
    }

    /// The last row of section `s` with an item in column `c`.
    fn last_row(&self, s: usize, c: usize) -> usize {
        let rows = self.sections[s].grid.rows;
        (0..rows).rev().find(|&r| self.at(s, r, c).is_some()).unwrap_or(0)
    }

    /// The last column of section `s` with an item in row `r`.
    fn last_col(&self, s: usize, r: usize) -> usize {
        let cols = self.sections[s].grid.cols;
        (0..cols).rev().find(|&c| self.at(s, r, c).is_some()).unwrap_or(0)
    }

    /// The item below item `i`: the next one in its column, or else the one
    /// at the top of the next section (in the same column, if it has as
    /// many). After the last section comes the first one, if `wrap`.
    fn down(&self, i: usize, wrap: bool) -> Option<usize> {
        let (s, r, c) = self.cell(i);
        if r < self.last_row(s, c) {
            return self.at(s, r + 1, c);
        }
        let next = if s + 1 < self.sections.len() { s + 1 } else { 0 };
        if next <= s && !wrap {
            return None;
        }
        self.at(next, 0, c.min(self.last_col(next, 0)))
    }

    /// The item above item `i`, as [`Self::down`] gives the one below.
    fn up(&self, i: usize, wrap: bool) -> Option<usize> {
        let (s, r, c) = self.cell(i);
        if r > 0 {
            return self.at(s, r - 1, c);
        }
        let prev = if s > 0 { s - 1 } else { self.sections.len() - 1 };
        if prev >= s && !wrap {
            return None;
        }
        let c = c.min(self.sections[prev].grid.cols - 1);
        self.at(prev, self.last_row(prev, c), c)
    }

    /// The item a screenful from item `i` in the direction `step` goes, or
    /// from the last one that way, the first one again.
    fn page(&self, i: usize, step: impl Fn(usize, bool) -> Option<usize>) -> usize {
        let mut at = i;
        for _ in 0..self.shown.max(1) {
            match step(at, false) {
                Some(next) => at = next,
                None => break,
            }
        }
        if at == i { step(i, true).unwrap_or(i) } else { at }
    }
}

/// The SGR parameters for the menu (each may be empty).
#[derive(Default)]
pub struct Style {
    pub select: String,
    pub desc: String,
    /// The start of the names, which the matches have in common.
    pub prefix: String,
    /// The headings of the groups.
    pub group: String,
    pub dir: String,
    pub exec: String,
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
    /// The texts that all the matches are known for: the matches of any
    /// word that starts with one of them are among `all`. The first is the
    /// text the menu was opened for.
    bases: Vec<String>,
    /// All the matches, and the headings of their groups.
    all: Vec<Item>,
    groups: Vec<String>,
    /// The matches listed, as positions in `all`: those that match the text
    /// typed.
    listed: Vec<usize>,
    /// The match selected, as a position in `listed`.
    selected: Option<usize>,
    /// A move asked for by a key binding, which the next completion makes.
    pub(super) pending: Option<Move>,
    /// The layout the menu was last drawn with, for moving in it.
    layout: Layout,
    /// The first row shown.
    top: usize,
}

impl Menu {
    /// Opens the menu with `matches`, which complete the text of `line`
    /// from `start` to the cursor at `pos`.
    pub fn open(&mut self, line: &str, start: usize, pos: usize, matches: Matches) {
        let typed = line.get(start..pos).unwrap_or_default();
        *self = Menu {
            open: true,
            line: line.to_owned(),
            pos,
            start,
            typed: typed.to_owned(),
            bases: vec![typed.to_owned()],
            listed: (0..matches.items.len()).collect(),
            all: matches.items,
            groups: matches.groups,
            ..Menu::default()
        };
    }

    pub fn close(&mut self) {
        *self = Menu::default();
    }

    pub fn is_open(&self, line: &str, pos: usize) -> bool {
        self.open && self.pos == pos && self.line == line
    }

    fn item(&self, i: usize) -> &Item {
        &self.all[self.listed[i]]
    }

    /// Follows the word as it is typed while the menu lists the matches
    /// (none is selected): if `line` and `pos` are the line and cursor the
    /// menu is open for but for the text typed, lists the matches for that
    /// text and gives true. The menu doesn't follow (and is as it was, for
    /// the line to come back to) once nothing matches, the one match is
    /// typed in full, or the text is no longer one the matches are known
    /// for with more typed after it.
    pub fn follow(&mut self, line: &str, pos: usize) -> bool {
        if !self.open || self.selected.is_some() || pos < self.start {
            return false;
        }
        let (Some(head), Some(tail)) = (line.get(..self.start), line.get(pos..)) else {
            return false;
        };
        if Some(head) != self.line.get(..self.start) || Some(tail) != self.line.get(self.pos..) {
            return false;
        }
        let typed = &line[self.start..pos];
        if !self.bases.iter().any(|b| typed.starts_with(&b[..])) {
            return false;
        }
        let listed: Vec<usize> = if typed == self.bases[0] {
            (0..self.all.len()).collect()
        } else {
            narrowed(&self.all, typed)
        };
        match listed[..] {
            [] => return false,
            [only] if self.all[only].replacement == typed => return false,
            _ => {}
        }
        self.listed = listed;
        self.typed = typed.to_owned();
        self.line = line.to_owned();
        self.pos = pos;
        self.top = 0;
        true
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
            Move::Next if self.selected.is_none() && self.listed.len() == 1 => {
                // The one match left of those listed: Tab completes it.
                self.open = false;
                self.item(0).replacement.clone()
            }
            Move::Next if self.selected.is_none() && self.common().len() > self.typed.len() => {
                // As Tab does before it opens the menu: what the matches
                // listed have in common comes first. (They are all the
                // matches for it.)
                let prefix = self.common().to_owned();
                self.bases.push(prefix.clone());
                self.typed.clone_from(&prefix);
                prefix
            }
            m => {
                let i = self.moved(m);
                self.selected = Some(i);
                self.item(i).replacement.clone()
            }
        };
        self.line = [&line[..self.start], &text, &line[pos..]].concat();
        self.pos = self.start + text.len();
        Some((self.start, text))
    }

    /// What the replacements of the matches listed have in common.
    fn common(&self) -> &str {
        common_prefix(self.listed.iter().map(|&i| &self.all[i].replacement[..]))
    }

    /// The item that move `m` selects.
    fn moved(&self, m: Move) -> usize {
        let n = self.listed.len();
        let Some(i) = self.selected else {
            return match m {
                Move::Prev | Move::Up | Move::Left | Move::PageUp => n - 1,
                _ => 0,
            };
        };
        // (The layout is the one drawn, unless nothing was drawn yet.)
        let l = if self.layout.len == n {
            &self.layout
        } else {
            let items: Vec<&Item> = self.listed.iter().map(|&i| &self.all[i]).collect();
            &Layout::new(&items, &self.groups, usize::MAX, usize::MAX)
        };
        let (s, r, c) = l.cell(i);
        let one_column = l.sections[s].grid.cols == 1;
        match m {
            Move::Next => (i + 1) % n,
            Move::Prev => (i + n - 1) % n,
            Move::Right if one_column => (i + 1) % n,
            Move::Left if one_column => (i + n - 1) % n,
            Move::Down => l.down(i, true).unwrap_or(i),
            Move::Up => l.up(i, true).unwrap_or(i),
            Move::Right if c < l.last_col(s, r) => l.at(s, r, c + 1).unwrap_or(i),
            Move::Right => l.at(s, r, 0).unwrap_or(i),
            Move::Left if c > 0 => l.at(s, r, c - 1).unwrap_or(i),
            Move::Left => l.at(s, r, l.last_col(s, r)).unwrap_or(i),
            Move::PageDown => l.page(i, |i, wrap| l.down(i, wrap)),
            Move::PageUp => l.page(i, |i, wrap| l.up(i, wrap)),
            Move::Cancel => i,
        }
    }

    /// Draws the menu for a terminal `cols` wide, with `avail` rows below
    /// the line. Each row starts with a newline.
    pub fn draw(&mut self, cols: usize, avail: usize, style: &Style) -> String {
        let items: Vec<&Item> = self.listed.iter().map(|&i| &self.all[i]).collect();
        let l = Layout::new(&items, &self.groups, cols, avail);
        if let Some(i) = self.selected {
            let (s, r, _) = l.cell(i);
            let sec = &l.sections[s];
            // The first row of a section brings its heading with it.
            let first = sec.row + r - usize::from(r == 0 && sec.heading.is_some());
            if first < self.top {
                self.top = first;
            } else if sec.row + r >= self.top + l.shown {
                self.top = sec.row + r + 1 - l.shown;
            }
        }
        self.top = self.top.min(l.rows - l.shown);
        let rows = self.top..self.top + l.shown;
        let usable = cols.saturating_sub(1).max(1);
        // What the names have in common is what is typed (or what Tab
        // would add to it), and what comes after it tells them apart.
        let shade = match items[..] {
            [] | [_] => 0,
            _ => common_prefix(items.iter().map(|i| &i.display[..])).len(),
        };
        let name = |out: &mut String, item: &Item, name: &str| {
            let k = if name.is_char_boundary(shade) { shade } else { 0 };
            styled(out, &style.prefix, &name[..k]);
            let kind = match item.kind {
                Kind::Plain => "",
                Kind::Dir => &style.dir,
                Kind::Exec => &style.exec,
            };
            styled(out, kind, &name[k..]);
        };
        let mut out = String::new();
        for sec in &l.sections {
            let g = &sec.grid;
            if let Some(h) = (sec.heading.as_ref()).filter(|_| sec.row > 0 && rows.contains(&(sec.row - 1))) {
                out.push('\n');
                styled(&mut out, &style.group, &truncate(&format!("{h}:"), usable).0);
            }
            for r in (0..g.rows).filter(|r| rows.contains(&(sec.row + r))) {
                out.push('\n');
                for c in 0..g.cols {
                    let i = g.index(r, c);
                    let Some(item) = items.get(sec.first + i).filter(|_| i < sec.len) else {
                        break;
                    };
                    let selected = self.selected == Some(sec.first + i);
                    if c > 0 {
                        out.push_str("  ");
                    }
                    if g.descs {
                        let (text, w) = truncate(&item.display, usable);
                        let pad = g.widths[0].saturating_sub(w);
                        let lead = " ".repeat((pad + 2).min(usable - w));
                        let desc =
                            (item.desc.as_ref()).map(|d| truncate(&format!("-- {d}"), usable - w - lead.len()).0);
                        match desc {
                            Some(desc) if selected => styled(&mut out, &style.select, &format!("{text}{lead}{desc}")),
                            Some(desc) => {
                                name(&mut out, item, &text);
                                out.push_str(&lead);
                                styled(&mut out, &style.desc, &desc);
                            }
                            None if selected => styled(&mut out, &style.select, &format!("{text}{:pad$}", "")),
                            None => name(&mut out, item, &text),
                        }
                        continue;
                    }
                    let (text, w) = truncate(&item.display, g.widths[c]);
                    let last = c + 1 == g.cols || g.index(r, c + 1) >= sec.len;
                    let pad = if selected || !last { g.widths[c] - w } else { 0 };
                    if selected {
                        styled(&mut out, &style.select, &format!("{text}{:pad$}", ""));
                    } else {
                        name(&mut out, item, &text);
                        out.push_str(&" ".repeat(pad));
                    }
                }
            }
        }
        if l.shown < l.rows && l.shown > 0 {
            let mut at = format!("rows {}-{} of {}", self.top + 1, self.top + l.shown, l.rows);
            if l.rows != items.len() {
                at.push_str(&format!(", {} matches", items.len()));
            }
            out.push('\n');
            styled(&mut out, &style.desc, &truncate(&at, usable).0);
        }
        self.layout = l;
        out
    }
}

/// The items of `all` (as positions) that match when `typed` is the text
/// typed: those whose replacements start with it, or else those that do
/// when a lowercase letter typed stands for either case (as the completer
/// matches, where an exact match also leaves the others out).
fn narrowed(all: &[Item], typed: &str) -> Vec<usize> {
    let typed = typed.as_bytes();
    let filter = |matches: &dyn Fn(&[u8]) -> bool| -> Vec<usize> {
        let all = all.iter().enumerate();
        all.filter(|(_, i)| matches(i.replacement.as_bytes()))
            .map(|(k, _)| k)
            .collect()
    };
    let exact = filter(&|r| r.starts_with(typed));
    if !exact.is_empty() {
        return exact;
    }
    let same = |r: &u8, t: &u8| r == t || (t.is_ascii_lowercase() && *r == t.to_ascii_uppercase());
    filter(&|r| r.len() >= typed.len() && r.iter().zip(typed).all(|(r, t)| same(r, t)))
}

/// The longest common prefix of `texts`.
pub fn common_prefix<'a>(mut texts: impl Iterator<Item = &'a str>) -> &'a str {
    let Some(first) = texts.next() else { return "" };
    let mut n = first.len();
    for t in texts {
        n = n.min(first.bytes().zip(t.bytes()).take_while(|(a, b)| a == b).count());
    }
    while !first.is_char_boundary(n) {
        n -= 1;
    }
    &first[..n]
}

/// How many rows the menu may take on a terminal with `rows` rows, `used`
/// of them by the prompt and the line: half the screen (but 8 rows on a
/// small one), so that what is above the prompt stays in sight, or the rows
/// below the line if they are fewer.
pub fn height(rows: usize, used: usize) -> usize {
    rows.saturating_sub(used).min((rows / 2).max(8))
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
pub(super) fn truncate(s: &str, max: usize) -> (String, usize) {
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
    let (row, col) = position(text, cols);
    row + 1 + usize::from(col == cols)
}

/// Where `text` ends on a terminal `cols` wide, as (row, column), counted as
/// in [`rows`].
pub fn position(text: &str, cols: usize) -> (usize, usize) {
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
    (row, col)
}

/// What is drawn after the line, as rustyline's hint: the menu, or an
/// autosuggestion.
pub struct Drawn {
    pub display: String,
    /// The text that accepting the hint inserts: none for the menu, so that
    /// moving right at the end of the line doesn't insert it.
    pub completion: Option<String>,
}

impl Hint for Drawn {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        self.completion.as_deref()
    }
}

#[derive(Clone, Copy)]
enum Action {
    Move(Move),
    /// Keeps the selected match and closes the menu.
    Accept,
}

/// The keys that act on the menu while it is open (and otherwise do what
/// they normally do).
const KEYS: &[(KeyEvent, Action)] = {
    use KeyCode as K;
    const fn key(k: KeyCode) -> KeyEvent {
        KeyEvent(k, Modifiers::NONE)
    }
    const fn ctrl(c: char) -> KeyEvent {
        KeyEvent(KeyCode::Char(c), Modifiers::CTRL)
    }
    &[
        (key(K::BackTab), Action::Move(Move::Prev)),
        (key(K::Down), Action::Move(Move::Down)),
        (key(K::Up), Action::Move(Move::Up)),
        (key(K::Right), Action::Move(Move::Right)),
        (key(K::Left), Action::Move(Move::Left)),
        (ctrl('N'), Action::Move(Move::Down)),
        (ctrl('P'), Action::Move(Move::Up)),
        (ctrl('F'), Action::Move(Move::Right)),
        (ctrl('B'), Action::Move(Move::Left)),
        (key(K::PageDown), Action::Move(Move::PageDown)),
        (key(K::PageUp), Action::Move(Move::PageUp)),
        (key(K::Esc), Action::Move(Move::Cancel)),
        (ctrl('G'), Action::Move(Move::Cancel)),
        (key(K::Enter), Action::Accept),
    ]
};

/// The keys the menu uses, which the key bindings (`keys.rs`) give to
/// [`key`] first.
pub fn keys() -> Vec<KeyEvent> {
    KEYS.iter().map(|k| k.0).collect()
}

/// Acts on the menu for `key`, if it is open and the key is one of its
/// own; otherwise returns None, and the key does what it normally does.
/// The arrow keys, other than Down, move only once a match is selected, so
/// they go on moving in the line and the history while the menu only lists
/// the matches.
pub fn key(menu: &Mutex<Menu>, key: KeyEvent, ctx: &EventContext) -> Option<Cmd> {
    let action = KEYS.iter().find(|k| k.0 == key)?.1;
    let mut menu = menu.lock().ok()?;
    if !menu.is_open(ctx.line(), ctx.pos()) {
        return None;
    }
    let selecting = menu.selected.is_some();
    match action {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn items(names: &[&str]) -> Vec<Item> {
        (names.iter())
            .map(|n| Item {
                display: n.to_string(),
                replacement: format!("{n} "),
                ..Item::default()
            })
            .collect()
    }

    /// Items in groups: each is (heading, names).
    fn grouped(groups: &[(&str, &[&str])]) -> Matches {
        let mut m = Matches::default();
        for (k, (heading, names)) in groups.iter().enumerate() {
            m.groups.push(heading.to_string());
            m.items
                .extend(items(names).into_iter().map(|i| Item { group: k as u16, ..i }));
        }
        m
    }

    fn plain() -> Style {
        Style {
            select: "7".into(),
            ..Style::default()
        }
    }

    fn widths(names: &[&str]) -> Vec<usize> {
        names.iter().map(|n| width(n)).collect()
    }

    #[test]
    fn grid() {
        let names: Vec<String> = (0..10).map(|i| format!("item{i}")).collect();
        let names: Vec<&str> = names.iter().map(|s| &s[..]).collect();
        let layout = |cols, avail| Layout::new(&items(&names).iter().collect::<Vec<_>>(), &[], cols, avail);
        // Four columns of 5 fit in 30 columns; 10 items take 3 rows, and
        // then 4 columns.
        let l = layout(30, 10);
        let g = &l.sections[0].grid;
        assert_eq!((g.rows, g.cols, g.by_column, l.shown), (3, 4, true, 3));
        assert_eq!(g.widths, [5, 5, 5, 5]);
        assert_eq!((g.index(2, 1), g.cell(5)), (5, (2, 1)));
        // Too many rows for the screen: along the rows, and scrolling.
        let l = layout(30, 2);
        let g = &l.sections[0].grid;
        assert_eq!((g.rows, g.cols, g.by_column, l.shown), (3, 4, false, 1));
        assert_eq!((g.index(2, 1), g.cell(5)), (9, (1, 1)));
        // Each column as wide as its widest item.
        let g = Grid::new(&widths(&["a", "b", "long name", "c"]), false, 15, true);
        assert_eq!((g.rows, g.cols, &g.widths[..]), (2, 2, &[1, 9][..]));
        // Descriptions: one per row.
        let g = Grid::new(&widths(&["a", "bb"]), true, 79, true);
        assert_eq!((g.rows, g.cols, &g.widths[..]), (2, 1, &[2][..]));
        // A name much longer than the others doesn't push their
        // descriptions to the right.
        let mut names = vec!["--all"; 19];
        names.push("--a-very-long-option-that-goes-on-and-on");
        let g = Grid::new(&widths(&names), true, 79, true);
        assert_eq!(g.widths, [5]);
    }

    #[test]
    fn menu_height() {
        // Half the screen, but 8 rows on a small one, if there are as many
        // below the line.
        assert_eq!(height(24, 1), 12);
        assert_eq!(height(50, 3), 25);
        assert_eq!(height(12, 1), 8);
        assert_eq!(height(12, 10), 2);
        assert_eq!(height(24, 30), 0);
    }

    #[test]
    fn moves() {
        let mut m = Menu::default();
        m.open("ls x", 3, 4, items(&["a", "b", "c", "d", "e"]).into());
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
        assert_eq!(go(Move::PageUp), "a ");
        assert_eq!(go(Move::PageUp), "c ");
        assert_eq!(go(Move::PageDown), "a ");
        assert_eq!(go(Move::PageDown), "c ");
        assert_eq!(go(Move::Cancel), "x");
        assert!(!m.open);
        // Before anything is selected.
        m.open("ls x", 3, 4, items(&["a", "b", "c"]).into());
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
        m.open(
            "ls ",
            3,
            3,
            items(&["alpha", "beta", "gamma", "delta", "epsilon"]).into(),
        );
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
        // The last row says which are shown (here cut to the width).
        assert_eq!(
            m.draw(18, 2, &style),
            "\n\x1b[7mgamma  \x1b[0m  delta\nrows 2-2 of 3, 5…"
        );
        m.selected = Some(4);
        assert_eq!(m.draw(18, 2, &style), "\n\x1b[7mepsilon\x1b[0m\nrows 3-3 of 3, 5…");
        assert_eq!(m.draw(18, 0, &style), "");
        // Descriptions, aligned and cut to the width.
        let mut it = items(&["add", "commit", "x"]);
        it[0].desc = Some("Add files".into());
        it[1].desc = Some("Record changes to the repository".into());
        m.open("git ", 4, 4, it.into());
        let style = Style {
            select: "7".into(),
            desc: "90".into(),
            ..Style::default()
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
        // One per row, the rows shown say which they are.
        assert_eq!(
            m.draw(30, 2, &style),
            "\n\x1b[7madd     -- Add files\x1b[0m\n\x1b[90mrows 1-1 of 3\x1b[0m"
        );
    }

    #[test]
    fn colours() {
        // What the names have in common is shaded, and directories and
        // executables are in their colours, unless selected.
        let mut it = items(&["src/", "script", "spec"]);
        it[0].kind = Kind::Dir;
        it[1].kind = Kind::Exec;
        let mut m = Menu::default();
        m.open("ls s", 3, 4, it.into());
        let style = Style {
            select: "7".into(),
            prefix: "2".into(),
            dir: "34".into(),
            exec: "32".into(),
            ..Style::default()
        };
        assert_eq!(
            m.draw(40, 10, &style),
            "\n\x1b[2ms\x1b[0m\x1b[34mrc/\x1b[0m  \x1b[2ms\x1b[0m\x1b[32mcript\x1b[0m  \x1b[2ms\x1b[0mpec"
        );
        m.step("ls s", 4);
        assert_eq!(
            m.draw(40, 10, &style),
            "\n\x1b[7msrc/\x1b[0m  \x1b[2ms\x1b[0m\x1b[32mcript\x1b[0m  \x1b[2ms\x1b[0mpec"
        );
        // A name cut before the end of the common part isn't shaded.
        let long = [
            "a-long-common-start-1",
            "a-long-common-start-2",
            "a-long-common-start-3",
        ];
        m.open("ls ", 3, 3, items(&long).into());
        assert_eq!(m.draw(12, 10, &style), "\na-long-com…\na-long-com…\na-long-com…");
    }

    #[test]
    fn groups() {
        let matches = grouped(&[
            ("aliases", &["ga", "gb"]),
            ("", &["gx"]),
            ("commands", &["gc", "gd", "ge"]),
        ]);
        let style = Style {
            select: "7".into(),
            group: "1".into(),
            ..Style::default()
        };
        let mut m = Menu::default();
        m.open("g", 0, 1, matches.clone());
        // A heading for each group that has one.
        let all = "\n\x1b[1maliases:\x1b[0m\nga  gb\ngx\n\x1b[1mcommands:\x1b[0m\ngc  gd  ge";
        assert_eq!(m.draw(40, 10, &style), all);
        let go = |m: &mut Menu, mv| {
            m.pending = Some(mv);
            m.step(&m.line.clone(), m.pos).unwrap().1
        };
        // Down and Up go from a group to the next, in the same column if
        // it has one.
        let moves = [
            (Move::Next, "ga "),
            (Move::Right, "gb "),
            (Move::Down, "gx "),
            (Move::Down, "gc "),
            (Move::Right, "gd "),
            (Move::Right, "ge "),
            (Move::Right, "gc "),
            (Move::Left, "ge "),
            (Move::Down, "gb "),
            (Move::Up, "gd "),
            (Move::Up, "gx "),
            (Move::Up, "ga "),
            (Move::Up, "gc "),
            (Move::Next, "gd "),
            (Move::PageUp, "ga "),
            (Move::PageDown, "gc "),
        ];
        for (mv, want) in moves {
            assert_eq!(go(&mut m, mv), want, "{mv:?}");
        }
        // Scrolling: the first row of a group comes with its heading.
        m.open("g", 0, 1, matches.clone());
        assert_eq!(
            m.draw(40, 3, &style),
            "\n\x1b[1maliases:\x1b[0m\nga  gb\nrows 1-2 of 5, 6 matches"
        );
        assert_eq!(go(&mut m, Move::Prev), "ge ");
        assert_eq!(
            m.draw(40, 3, &style),
            "\n\x1b[1mcommands:\x1b[0m\ngc  gd  \x1b[7mge\x1b[0m\nrows 4-5 of 5, 6 matches"
        );
        assert_eq!(go(&mut m, Move::Up), "gx ");
        assert_eq!(
            m.draw(40, 3, &style),
            "\n\x1b[7mgx\x1b[0m\n\x1b[1mcommands:\x1b[0m\nrows 3-4 of 5, 6 matches"
        );
        assert_eq!(go(&mut m, Move::Up), "ga ");
        assert_eq!(
            m.draw(40, 3, &style),
            "\n\x1b[1maliases:\x1b[0m\n\x1b[7mga\x1b[0m  gb\nrows 1-2 of 5, 6 matches"
        );
        // The matches of one group are listed without its heading.
        m.open("g", 0, 1, matches);
        assert!(m.follow("gc", 2));
        assert_eq!(m.draw(40, 10, &style), "\ngc");
    }

    #[test]
    fn narrowing() {
        let style = plain();
        let names = ["Cargo.lock", "Cargo.toml", "README.md", "readme.txt", "src"];
        let mut m = Menu::default();
        m.open("ls ", 3, 3, items(&names).into());
        // What is typed after the menu is opened narrows it.
        assert!(m.follow("ls C", 4));
        assert!(m.is_open("ls C", 4));
        assert_eq!(m.draw(40, 10, &style), "\nCargo.lock  Cargo.toml");
        // Tab then adds what the matches left have in common, and selects
        // only after that.
        assert_eq!(m.step("ls C", 4), Some((3, "Cargo.".into())));
        assert_eq!(m.draw(40, 10, &style), "\nCargo.lock  Cargo.toml");
        assert!(m.follow("ls Cargo.t", 10));
        assert_eq!(m.draw(40, 10, &style), "\nCargo.toml");
        // Deleting brings the others back, as far as the text the menu was
        // opened for.
        assert!(m.follow("ls Cargo.", 9));
        assert!(m.follow("ls ", 3));
        assert_eq!(m.listed.len(), 5);
        // A lowercase letter matches either case, unless a name has it.
        assert!(m.follow("ls r", 4));
        assert_eq!(m.draw(40, 10, &style), "\nreadme.txt");
        assert!(m.follow("ls R", 4));
        assert_eq!(m.draw(40, 10, &style), "\nREADME.md");
        assert!(m.follow("ls c", 4));
        assert_eq!(m.draw(40, 10, &style), "\nCargo.lock  Cargo.toml");
        // The one match left is completed by Tab, which closes the menu.
        assert!(m.follow("ls s", 4));
        assert_eq!(m.step("ls s", 4), Some((3, "src ".into())));
        assert!(!m.is_open("ls src ", 7));
        // Nothing matches, or the match is typed in full: the menu is as
        // it was, for when the line is again.
        m.open("ls ", 3, 3, items(&names).into());
        assert!(m.follow("ls sr", 5));
        assert!(!m.follow("ls srx", 6));
        assert!(!m.follow("ls src ", 7));
        assert!(m.is_open("ls sr", 5));
        // Not the same line but for the word, or before the word.
        assert!(!m.follow("la sr", 5));
        assert!(!m.follow("ls sr; x", 5));
        assert!(!m.follow("ls", 2));
        // With text after the cursor.
        m.open("ls  | wc", 3, 3, items(&names).into());
        assert!(m.follow("ls sr | wc", 5));
        assert!(!m.follow("ls sr | wc -l", 5));
        // A text shorter than the one the matches are for isn't followed.
        m.open("ls Ca", 3, 5, items(&names[..2]).into());
        assert!(!m.follow("ls C", 4));
        assert!(m.is_open("ls Ca", 5));
        // Nor is anything once a match is selected.
        m.step("ls Ca", 5);
        assert!(!m.follow("ls Cargo.lock x", 15));
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
