//! `jobs -i`: a menu of the jobs, to bring one to the foreground, continue
//! it in the background, stop it, disown it (as `disown -h` does, which the
//! menu can undo), or send it a signal that ends it.
//!
//! The signals that end a job take two keys: `K` lists them, and the next
//! key chooses one (any other key cancels), so that a stray key can't end a
//! job. One of them sends TERM, then KILL if the job hasn't ended
//! [`ESCALATE`] later; the menu stays open until then (leaving it waits, or
//! Ctrl-C leaves without the KILL), as nothing would send it once the line
//! editor has the terminal.
//!
//! The menu is drawn on stderr below the command, as the first run's is
//! (`firstrun.rs`), and erased when it is left. The loop (`run`) reaps
//! children while it waits for keys, so the menu shows jobs as they change.
//! The jobs stay in the table while it is open (none is freed), so a
//! finished job is reported at the next prompt as usual. `Menu` handles the keys and draws, without `Shell`, so that it
//! can be tested alone; the colours are the `jobs.*` styles ([`Colours`]).

use std::time::{Duration, Instant};

use super::menu::truncate;
use super::tty::{Raw, read_byte, readable};
use crate::jobs::{JobState, JobTable};
use crate::shell::Shell;
use crate::style::Role;
use crate::{signals, sys};

/// How long after TERM the menu sends KILL, for the `e` choice.
pub const ESCALATE: Duration = Duration::from_secs(5);

/// The choices after `K`: the key, the signal (0 for TERM then KILL), and
/// what it does.
const SIGNALS: &[(u8, i32, &str)] = &[
    (b't', libc::SIGTERM, "TERM: ask it to end"),
    (b'e', 0, "TERM, then KILL if it hasn't ended in"),
    (b'k', libc::SIGKILL, "KILL: end it at once"),
    (b'i', libc::SIGINT, "INT: as Ctrl-C"),
    (b'h', libc::SIGHUP, "HUP: as when the terminal closes"),
];

/// The keys, and what they do. `s` continues a stopped job instead, and
/// `d` undoes the disowning of a disowned one.
const HELP: &[(&str, &str)] = &[
    ("f", "foreground"),
    ("b", "background"),
    ("s", "stop"),
    ("d", "disown"),
    ("K", "kill..."),
    ("q", "quit"),
];

/// The SGR parameters of the parts of the menu (empty for plain text).
#[derive(Clone, Debug)]
struct Colours {
    selected: String,
    number: String,
    pid: String,
    running: String,
    stopped: String,
    done: String,
    key: String,
}

impl Default for Colours {
    /// No colours, but the selection in reverse video.
    fn default() -> Colours {
        let plain = String::new;
        Colours {
            selected: "7".into(),
            number: plain(),
            pid: plain(),
            running: plain(),
            stopped: plain(),
            done: plain(),
            key: plain(),
        }
    }
}

impl Colours {
    /// The `jobs.*` styles (and `menu.selected`) of the colour scheme in
    /// use, or none with `$NO_COLOR`. The selection is in reverse video if
    /// the scheme gives it no style.
    fn new(sh: &Shell) -> Colours {
        if sh.get_var(b"NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return Colours::default();
        }
        let scheme = crate::builtins::style::scheme_in_use(sh);
        let r = sh.styles.resolver(scheme.as_deref());
        let get = |role: Role| {
            role.index()
                .map_or(String::new(), |i| r.get(crate::style::ROLES[i]).sgr())
        };
        let selected = get(Role::of("menu.selected"));
        Colours {
            selected: if selected.is_empty() { "7".into() } else { selected },
            number: get(Role::of("jobs.number")),
            pid: get(Role::of("jobs.pid")),
            running: get(Role::of("jobs.running")),
            stopped: get(Role::of("jobs.stopped")),
            done: get(Role::of("jobs.done")),
            key: get(Role::of("jobs.key")),
        }
    }

    fn state(&self, state: JobState) -> &str {
        match state {
            JobState::Running => &self.running,
            JobState::Stopped => &self.stopped,
            JobState::Done => &self.done,
        }
    }
}

/// `parts` (each a text in its SGR parameters) cut to `max` columns as
/// [`truncate`] cuts. With SGR parameters `base` (the selection's), the
/// whole line is in them, as wide as `max`.
fn paint(parts: &[(&str, &str)], base: &str, max: usize) -> String {
    let plain: String = parts.iter().map(|p| p.1).collect();
    let (text, w) = truncate(&plain, max);
    let mut chars = text.chars();
    let mut out = String::new();
    if !base.is_empty() {
        out.push_str(&format!("\x1b[{base}m"));
    }
    for (i, &(sgr, part)) in parts.iter().enumerate() {
        // The last part gets the `…` of a cut in it.
        let n = if i + 1 == parts.len() {
            usize::MAX
        } else {
            part.chars().count()
        };
        let shown: String = chars.by_ref().take(n).collect();
        match (sgr, base) {
            _ if shown.is_empty() => {}
            ("", _) => out.push_str(&shown),
            (_, "") => out.push_str(&format!("\x1b[{sgr}m{shown}\x1b[m")),
            _ => out.push_str(&format!("\x1b[{sgr}m{shown}\x1b[0;{base}m")),
        }
    }
    if !base.is_empty() {
        out.push_str(&format!("{:pad$}\x1b[m", "", pad = max - w));
    }
    out
}

/// A job, as the menu shows it.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    number: usize,
    mark: char,
    pid: i32,
    kind: JobState,
    state: String,
    /// It was disowned, so it doesn't get SIGHUP when the shell exits.
    nohup: bool,
    cmd: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    Enter,
    Char(u8),
    /// Ctrl-C, Ctrl-D, Esc, or the end of the input.
    Cancel,
    Other,
}

/// What a key asks for, for the selected job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    Quit,
    Foreground,
    Background,
    /// Stop a running job, or continue a stopped one in the background.
    Stop,
    /// Disown the job as `disown -h` does, or undo that.
    Disown,
    /// A signal that ends the job.
    Signal(i32),
    /// TERM, then KILL after [`ESCALATE`].
    TermThenKill,
}

#[derive(Default)]
struct Menu {
    colours: Colours,
    /// The selected row.
    sel: usize,
    /// `K` was pressed: the next key chooses a signal.
    kill: bool,
    /// What the last key did, or why it couldn't.
    message: String,
    /// The first row shown.
    top: usize,
}

impl Menu {
    /// Acts on `key`, with the jobs `rows`.
    fn key(&mut self, key: Key, rows: &[Row]) -> Action {
        self.message.clear();
        if std::mem::take(&mut self.kill) {
            let chosen = SIGNALS.iter().find(|s| key == Key::Char(s.0));
            return match chosen.map(|s| s.1) {
                Some(0) => Action::TermThenKill,
                Some(sig) => Action::Signal(sig),
                None => {
                    self.message = "Nothing sent".into();
                    Action::None
                }
            };
        }
        let n = rows.len();
        match key {
            _ if n == 0 => return Action::Quit,
            Key::Up | Key::Char(b'k' | 0x10) => self.sel = (self.sel + n - 1) % n,
            Key::Down | Key::Char(b'j' | 0x0e | b'\t') => self.sel = (self.sel + 1) % n,
            Key::Char(c @ b'1'..=b'9') => match rows.iter().position(|r| r.number == (c - b'0') as usize) {
                Some(i) => self.sel = i,
                None => self.message = format!("No job {}", c as char),
            },
            Key::Enter | Key::Char(b'f') => return Action::Foreground,
            Key::Char(b'b') => return Action::Background,
            Key::Char(b's') => return Action::Stop,
            Key::Char(b'd') => return Action::Disown,
            Key::Char(b'K') => self.kill = true,
            Key::Char(b'q') | Key::Cancel => return Action::Quit,
            _ => {}
        }
        Action::None
    }

    /// The lines of the menu for `rows`, on a terminal `cols` wide and
    /// `lines` high.
    fn draw(&mut self, rows: &[Row], cols: usize, lines: usize) -> Vec<String> {
        let usable = cols.saturating_sub(1).max(1);
        let c = &self.colours;
        let mut footer = Vec::new();
        let sel = rows.get(self.sel);
        match sel {
            Some(r) if self.kill => {
                footer.push(truncate(&format!("Send to [{}] {}:", r.number, r.cmd), usable).0);
                for &(key, sig, what) in SIGNALS {
                    let what = match sig {
                        0 => format!("  {what} {} s", ESCALATE.as_secs()),
                        _ => format!("  {what}"),
                    };
                    let key = (key as char).to_string();
                    footer.push(paint(&[("", "  "), (&c.key, &key), ("", &what)], "", usable));
                }
                footer.push(truncate("  (any other key: nothing)", usable).0);
            }
            _ => {
                if !self.message.is_empty() {
                    footer.push(truncate(&self.message, usable).0);
                }
                let stopped = sel.is_some_and(|r| r.kind == JobState::Stopped);
                let nohup = sel.is_some_and(|r| r.nohup);
                let mut parts = Vec::new();
                for (i, &(key, what)) in HELP.iter().enumerate() {
                    let what = match key {
                        "s" if stopped => "continue",
                        "d" if nohup => "undisown",
                        _ => what,
                    };
                    if i > 0 {
                        parts.push(("", "  "));
                    }
                    parts.extend([(c.key.as_str(), key), ("", ": "), ("", what)]);
                }
                footer.push(paint(&parts, "", usable));
            }
        }
        // The rows that fit, scrolled to the selection.
        let shown = lines.saturating_sub(footer.len() + 1).clamp(1, rows.len().max(1));
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + shown {
            self.top = self.sel + 1 - shown;
        }
        self.top = self.top.min(rows.len().saturating_sub(shown));
        let num_w = rows.iter().map(|r| r.number.to_string().len()).max().unwrap_or(1);
        let pid_w = rows.iter().map(|r| r.pid.to_string().len()).max().unwrap_or(1);
        let state_w = rows.iter().map(|r| r.state.len()).max().unwrap_or(0);
        let mut out = Vec::new();
        for (i, r) in rows.iter().enumerate().skip(self.top).take(shown) {
            let selected = i == self.sel;
            let num = format!("{:<w$}{}", format!("[{}]", r.number), r.mark, w = num_w + 2);
            let pid = format!("{:>pid_w$}", r.pid);
            let state = format!("{:<state_w$}", r.state);
            let parts = [
                ("", if selected { "> " } else { "  " }),
                (c.number.as_str(), &num),
                ("", " "),
                (&c.pid, &pid),
                ("", "  "),
                (c.state(r.kind), &state),
                ("", "  "),
                ("", &r.cmd),
            ];
            out.push(paint(&parts, if selected { &c.selected } else { "" }, usable));
        }
        out.extend(footer);
        out
    }
}

/// Reads a key from the terminal, in raw mode. An escape sequence is read
/// whole; Esc alone is one with nothing after it for 50 ms.
fn read_key() -> Key {
    match read_byte() {
        None | Some(3 | 4) => Key::Cancel,
        Some(b'\r' | b'\n') => Key::Enter,
        Some(0x1b) if !readable(50) => Key::Cancel,
        Some(0x1b) => match read_byte() {
            Some(b'[' | b'O') => {
                let mut last = read_byte();
                while let Some(b'0'..=b'9' | b';') = last {
                    last = read_byte();
                }
                match last {
                    Some(b'A') => Key::Up,
                    Some(b'B') => Key::Down,
                    _ => Key::Other,
                }
            }
            _ => Key::Other,
        },
        Some(c) => Key::Char(c),
    }
}

/// The menu's lines on the terminal, redrawn in place.
#[derive(Default)]
struct Screen {
    drawn: Vec<String>,
}

impl Screen {
    /// Goes back to the first line drawn, and clears from there.
    fn erase(&self) -> String {
        match self.drawn.len() {
            0 => "\r\x1b[J".into(),
            n => format!("\r\x1b[{n}A\x1b[J"),
        }
    }

    fn show(&mut self, lines: Vec<String>) {
        if lines == self.drawn {
            return;
        }
        let mut out = self.erase();
        for l in &lines {
            out.push_str(l);
            out.push_str("\r\n");
        }
        sys::write_all(2, out.as_bytes());
        self.drawn = lines;
    }
}

/// Whether the menu can be shown: on a terminal (standard input and
/// error) that can move the cursor.
pub fn usable(sh: &Shell) -> bool {
    let dumb = sh.get_var(b"TERM").is_none_or(|t| t.is_empty() || t == b"dumb");
    !dumb && sys::isatty(0) && sys::isatty(2)
}

/// A job sent TERM by the `e` choice.
struct Kill {
    slot: usize,
    /// When it gets KILL.
    at: Instant,
    /// KILL was sent (and the job hasn't been seen to end yet).
    sent: bool,
}

/// The jobs in `slots`, as the menu shows them, with the time left before
/// KILL for those in `kills`.
fn rows(sh: &Shell, slots: &[usize], kills: &[Kill], now: Instant) -> Vec<Row> {
    let row = |&i: &usize| {
        let job = sh.jobs.get(i);
        let mut state = job.state_text();
        match kills.iter().find(|k| k.slot == i) {
            Some(k) if k.sent => state.push_str(", sent KILL"),
            Some(k) => {
                let left = k.at.saturating_duration_since(now).as_millis().div_ceil(1000);
                state.push_str(&format!(", KILL in {left} s"));
            }
            None => {}
        }
        if job.nohup {
            state.push_str(", disowned");
        }
        // Without job control, jobs have no command text (as in dash).
        let mut cmd = job.text();
        if job.procs.iter().all(|p| p.cmd.is_empty()) {
            let pids: Vec<String> = job.procs.iter().map(|p| p.pid.to_string()).collect();
            cmd = format!("(pid {})", pids.join(" "));
        }
        Row {
            number: JobTable::number(i),
            mark: sh.jobs.mark(i).unwrap_or(' '),
            pid: job.pgid(),
            kind: job.state,
            state,
            nohup: job.nohup,
            cmd,
        }
    };
    slots.iter().map(row).collect()
}

/// Sends `sig` to job `i`, and continues it if it is stopped, so that it
/// can act on the signal (as bash and zsh do). Returns what to tell.
fn send(sh: &mut Shell, i: usize, sig: i32) -> String {
    let n = JobTable::number(i);
    if let Err(e) = sh.signal_job(i, sig) {
        return format!("[{n}]: {}", sys::strerror(e));
    }
    if sig != libc::SIGKILL && sh.jobs.get(i).state == JobState::Stopped {
        sh.restart_job(i, false);
    }
    format!("Sent {} to [{n}]", signals::name(sig))
}

/// Shows the menu of the jobs, with job `first` selected (by default the
/// current job), until it is left. Returns the status of `jobs -i`: that of
/// the job it brought to the foreground, if any, otherwise 0.
pub fn run(sh: &mut Shell, first: Option<usize>) -> i32 {
    sh.reap_jobs();
    let mut slots = sh.jobs.order().to_vec();
    slots.sort_unstable();
    let first = first.or_else(|| sh.jobs.order().first().copied());
    if slots.is_empty() {
        return 0;
    }
    let Some(raw) = Raw::new() else { return 0 };
    let mut menu = Menu {
        sel: slots.iter().position(|&s| Some(s) == first).unwrap_or(0),
        colours: Colours::new(sh),
        ..Menu::default()
    };
    let mut screen = Screen::default();
    // Leaving waits for these, so that the prompt reports them.
    let mut kills: Vec<Kill> = Vec::new();
    let mut leaving = false;
    sys::write_all(2, b"\x1b[?25l");
    let foreground = loop {
        sh.reap_jobs();
        let now = Instant::now();
        kills.retain(|k| sh.jobs.get(k.slot).state != JobState::Done);
        for k in kills.iter_mut().filter(|k| !k.sent && now >= k.at) {
            menu.message = send(sh, k.slot, libc::SIGKILL);
            k.sent = true;
        }
        if leaving && kills.is_empty() {
            break None;
        }
        let (cols, lines) = sys::window_size(2).unwrap_or((80, 24));
        let (cols, lines) = (if cols == 0 { 80 } else { cols }, if lines == 0 { 24 } else { lines });
        let rows = rows(sh, &slots, &kills, now);
        screen.show(menu.draw(&rows, cols, lines));
        // Wake up to show jobs that change, and the time left before KILL.
        if !readable(if kills.is_empty() { 500 } else { 100 }) {
            continue;
        }
        let key = read_key();
        if leaving {
            if matches!(key, Key::Cancel | Key::Char(b'q')) {
                kills.clear();
            }
            continue;
        }
        let action = menu.key(key, &rows);
        let slot = slots[menu.sel];
        let n = JobTable::number(slot);
        let job = sh.jobs.get(slot);
        let done = job.state == JobState::Done;
        let no_jobctl = !job.jobctl || !sh.jobctl();
        let message = match action {
            Action::None => continue,
            Action::Quit if kills.is_empty() => break None,
            Action::Quit => {
                leaving = true;
                "Waiting to send KILL (Ctrl-C: leave without sending it)".into()
            }
            _ if done => format!("[{n}] has ended"),
            Action::Foreground | Action::Background | Action::Stop | Action::Disown if no_jobctl => {
                format!("[{n}] wasn't started under job control")
            }
            Action::Foreground if !kills.is_empty() => "Waiting to send KILL: wait, or leave with q".into(),
            Action::Foreground => break Some(slot),
            Action::Background | Action::Stop if job.state == JobState::Stopped => {
                sh.jobs.make_current(slot, false);
                sh.restart_job(slot, false);
                format!("Continued [{n}] in the background")
            }
            Action::Background => format!("[{n}] is running"),
            Action::Stop => send(sh, slot, libc::SIGSTOP),
            Action::Disown => {
                let job = sh.jobs.get_mut(slot);
                job.nohup = !job.nohup;
                match job.nohup {
                    true => format!("Disowned [{n}]: it won't get HUP when the shell exits"),
                    false => format!("[{n}] is no longer disowned"),
                }
            }
            Action::Signal(sig) => send(sh, slot, sig),
            Action::TermThenKill => {
                if !kills.iter().any(|k| k.slot == slot) {
                    let at = Instant::now() + ESCALATE;
                    kills.push(Kill { slot, at, sent: false });
                }
                send(sh, slot, libc::SIGTERM)
            }
        };
        menu.message = message;
    };
    sys::write_all(2, [screen.erase(), "\x1b[?25h".into()].concat().as_bytes());
    drop(raw);
    let Some(slot) = foreground else { return 0 };
    // As `fg` does.
    let line = format!("{}\n", sh.jobs.get(slot).text());
    sh.out(line.as_bytes());
    sh.restart_job(slot, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        let row = |number, mark, pid, kind, state: &str, cmd: &str| Row {
            number,
            mark,
            pid,
            kind,
            state: state.into(),
            nohup: false,
            cmd: cmd.into(),
        };
        vec![
            row(1, ' ', 101, JobState::Running, "Running", "sleep 30"),
            row(2, '-', 2002, JobState::Stopped, "Stopped (signal)", "vi notes"),
            row(3, '+', 303, JobState::Running, "Running", "make | tee log"),
        ]
    }

    const HELP_RUNNING: &str = "f: foreground  b: background  s: stop  d: disown  K: kill...  q: quit";

    #[test]
    fn keys() {
        let rows = rows();
        let mut m = Menu::default();
        let mut moves = |k| {
            m.key(k, &rows);
            m.sel
        };
        assert_eq!(moves(Key::Up), 2);
        assert_eq!(moves(Key::Down), 0);
        assert_eq!(moves(Key::Char(b'j')), 1);
        assert_eq!(moves(Key::Char(b'3')), 2);
        assert_eq!(moves(Key::Char(b'7')), 2);
        assert_eq!(m.message, "No job 7");
        let keys = |m: &mut Menu, keys: &[Key]| keys.iter().map(|&k| m.key(k, &rows)).collect::<Vec<_>>();
        assert_eq!(
            keys(&mut m, &[Key::Enter, Key::Char(b'f'), Key::Char(b'b'), Key::Char(b's')]),
            [Action::Foreground, Action::Foreground, Action::Background, Action::Stop]
        );
        assert_eq!(keys(&mut m, &[Key::Char(b'd')]), [Action::Disown]);
        // A signal that ends a job takes two keys.
        assert_eq!(keys(&mut m, &[Key::Char(b'k')]), [Action::None]);
        assert_eq!(
            keys(&mut m, &[Key::Char(b'K'), Key::Char(b'k')]),
            [Action::None, Action::Signal(libc::SIGKILL)]
        );
        assert_eq!(
            keys(&mut m, &[Key::Char(b'K'), Key::Char(b'e')])[1],
            Action::TermThenKill
        );
        assert_eq!(
            keys(&mut m, &[Key::Char(b'K'), Key::Char(b't')])[1],
            Action::Signal(libc::SIGTERM)
        );
        // Any other key sends nothing, and the next is a key as usual.
        for other in [Key::Char(b'x'), Key::Char(b'q'), Key::Cancel, Key::Enter, Key::Down] {
            assert_eq!(keys(&mut m, &[Key::Char(b'K'), other]), [Action::None, Action::None]);
            assert_eq!(m.message, "Nothing sent");
        }
        assert_eq!(
            keys(&mut m, &[Key::Char(b'q'), Key::Cancel]),
            [Action::Quit, Action::Quit]
        );
    }

    #[test]
    fn draw() {
        let rows = rows();
        let mut m = Menu {
            sel: 1,
            ..Menu::default()
        };
        // The selected row is as wide as the screen, less the last column.
        assert_eq!(
            m.draw(&rows, 80, 24),
            [
                "  [1]   101  Running           sleep 30",
                &format!("\x1b[7m{:79}\x1b[m", "> [2]- 2002  Stopped (signal)  vi notes"),
                "  [3]+  303  Running           make | tee log",
                // The selected job is stopped: s continues it.
                "f: foreground  b: background  s: continue  d: disown  K: kill...  q: quit",
            ]
        );
        m.sel = 0;
        assert_eq!(m.draw(&rows, 80, 24)[3], HELP_RUNNING);
        // A disowned job: d undoes it.
        let mut disowned = rows.clone();
        disowned[0].nohup = true;
        assert!(m.draw(&disowned, 80, 24)[3].contains("  d: undisown  "));
        m.sel = 1;
        // `K`: the signals, for the selected job.
        m.key(Key::Char(b'K'), &rows);
        let lines = m.draw(&rows, 80, 24);
        assert_eq!(lines.len(), 3 + 1 + SIGNALS.len() + 1);
        assert_eq!(lines[3], "Send to [2] vi notes:");
        assert_eq!(lines[5], "  e  TERM, then KILL if it hasn't ended in 5 s");
        // A message; lines cut to the width.
        m.key(Key::Char(b'x'), &rows);
        let lines = m.draw(&rows, 20, 24);
        assert_eq!(&lines[3..], ["Nothing sent", "f: foreground  b: …"]);
        assert_eq!(lines[0], "  [1]   101  Runni…");
        // Too few lines: scrolled to the selection.
        m.message.clear();
        m.sel = 2;
        let lines = m.draw(&rows, 80, 4);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("  [2]-"), "{lines:?}");
        assert!(lines[1].contains("> [3]+"), "{lines:?}");
        m.sel = 0;
        assert!(m.draw(&rows, 80, 4)[0].contains("> [1]"));
    }

    #[test]
    fn colours() {
        let rows = rows();
        let mut m = Menu {
            colours: Colours {
                selected: "7".into(),
                number: "1".into(),
                pid: "90".into(),
                running: "32".into(),
                stopped: "33".into(),
                done: "90".into(),
                key: "1".into(),
            },
            ..Menu::default()
        };
        let lines = m.draw(&rows, 80, 24);
        // The state in its colour; the selected row on the selection's,
        // which each coloured part goes back to.
        let selected =
            "\x1b[7m> \x1b[1m[1] \x1b[0;7m \x1b[90m 101\x1b[0;7m  \x1b[32mRunning         \x1b[0;7m  sleep 30";
        assert_eq!(lines[0], format!("{selected}{:40}\x1b[m", ""));
        assert_eq!(
            lines[1],
            "  \x1b[1m[2]-\x1b[m \x1b[90m2002\x1b[m  \x1b[33mStopped (signal)\x1b[m  vi notes"
        );
        assert!(lines[3].starts_with("\x1b[1mf\x1b[m: foreground  \x1b[1mb\x1b[m: "));
        // Cut to the width, with the `…` in the colour of the part cut.
        assert_eq!(
            m.draw(&rows, 20, 24)[1],
            "  \x1b[1m[2]-\x1b[m \x1b[90m2002\x1b[m  \x1b[33mStopp…\x1b[m"
        );
        assert_eq!(paint(&[("1", "ab"), ("", "cd")], "", 3), "\x1b[1mab\x1b[m…");
    }
}
