//! Variable tracing: where each variable was last set (`setopt vars.trace`),
//! or the last changes of each (`setopt vars.trace_history`), which `where`
//! shows.
//!
//! While both options are off, `Shell::vartrace` is `None` and nothing is
//! recorded: the places that change variables only test it, and call the
//! (cold) functions here when it is set. They are `Shell::after_assign`
//! (every assignment that goes through `Shell`), `Shell::unset_var`,
//! `Shell::save_var` and `Shell::restore_saved` (`local` and temporary
//! assignments), and the few places that change values in `Vars` directly
//! (arithmetic assignments, `unset 'a[i]'`, `SHLVL`).
//!
//! Each change is recorded with where the code that made it is: its file
//! and line and the function running (`frames.rs`), or the prompt, the `-c`
//! command or standard input. Only the last change of a variable is kept,
//! except with `vars.trace_history` and for [`HISTORY_VARS`], whose last
//! [`HISTORY_LIMIT`] changes are kept with the values they set.
//!
//! When tracing starts, each variable that is set gets a first record: from
//! the environment (if it still has the value it had there), set by luish
//! as it started (for tracing turned on by `luish -o vars.trace`), or set
//! before tracing began. While tracing is on, the startup caches aren't used
//! (`startcache::Run::lookup`), so that what the startup files set is
//! recorded at their lines.

use std::collections::VecDeque;
use std::rc::Rc;

use crate::frames::{FrameKind, SourceFile};
use crate::hash::HashMap;
use crate::options::Opt;
use crate::shell::{ExecResult, Shell};
use crate::style::Role;
use crate::vars::{Special, Value};

/// How many changes of a variable are kept in history mode.
pub const HISTORY_LIMIT: usize = 100;

/// Variables whose changes are kept as in history mode whenever tracing is
/// on: those that startup files and plugins most often change bit by bit.
pub const HISTORY_VARS: &[&[u8]] = &[b"PATH", b"MANPATH", b"PS1", b"RPROMPT", b"RPS1"];

/// Values kept in a history are cut after this many bytes.
const MAX_VALUE: usize = 4096;

/// The record of what variables were set to, and where.
#[derive(Debug, Default)]
pub struct VarTrace {
    vars: HashMap<Vec<u8>, Changes>,
    /// For each variable put aside by `local` or a temporary assignment, the
    /// last change of the value put aside, innermost last, to tell where the
    /// value that `restore_saved` puts back was set.
    saved: HashMap<Vec<u8>, Vec<Option<Event>>>,
}

/// The changes of one variable, oldest first.
#[derive(Debug, Default)]
struct Changes {
    events: VecDeque<Event>,
    /// How many older changes were dropped (beyond [`HISTORY_LIMIT`]).
    dropped: usize,
}

#[derive(Debug, Clone)]
struct Event {
    what: What,
    /// Where the code that made the change was, for those made by code.
    at: Option<Box<Location>>,
    /// The value it set, as shown (`Some(None)` when unset), for a variable
    /// whose history is kept: `None` otherwise, as the current value is the
    /// one that its last change set.
    value: Option<Option<Box<[u8]>>>,
    /// For [`What::Restored`], the change that set the value put back, if
    /// it was recorded.
    origin: Option<Box<Event>>,
}

#[derive(Debug, Clone, PartialEq)]
enum What {
    Set,
    Unset,
    /// Put back when a function with it local returned (with the
    /// function's name), or after a command with a temporary assignment.
    Restored(Option<Rc<[u8]>>),
    /// In the environment, with the value it still had when tracing started.
    Environment,
    /// Set by luish as it started, before tracing started with it.
    Startup,
    /// Set before tracing started.
    Before,
}

#[derive(Debug, Clone)]
struct Location {
    place: Place,
    /// 0 if the line isn't known (or not a line of the file).
    line: u32,
    /// The function running.
    function: Option<Rc<[u8]>>,
}

#[derive(Debug, Clone)]
enum Place {
    File(Rc<SourceFile>),
    Prompt,
    Command,
    Stdin,
}

impl Shell {
    /// Starts or stops tracing after `vars.trace` or `vars.trace_history`
    /// changed. `at_startup` is for options given on the command line,
    /// where the variables that didn't come from the environment were set
    /// by luish.
    pub fn update_var_trace(&mut self, at_startup: bool) {
        let on = self.opt(Opt::VarsTrace) || self.opt(Opt::VarsTraceHistory);
        if on && self.vartrace.is_none() {
            self.vartrace = Some(Box::new(self.start_trace(at_startup)));
        } else if !on {
            self.vartrace = None;
        }
    }

    /// Sets an option, starting or stopping tracing if it is one of its own.
    pub fn set_option(&mut self, o: Opt, on: bool) {
        self.options.set(o, on);
        if matches!(o, Opt::VarsTrace | Opt::VarsTraceHistory) {
            self.update_var_trace(false);
        }
    }

    /// Restores the options that `set` sets to their values in `saved`, for
    /// `local -` (as dash; luish's own options, set with `setopt`, are kept,
    /// as bash keeps `shopt`'s).
    pub fn restore_options(&mut self, saved: &crate::options::Options) {
        for &(o, _, _) in crate::options::OPTIONS {
            if !matches!(o, Opt::Interactive | Opt::Stdin) {
                self.set_option(o, saved.get(o));
            }
        }
        self.set_jobctl(self.opt(Opt::Monitor));
    }

    fn start_trace(&self, at_startup: bool) -> VarTrace {
        use std::os::unix::ffi::OsStrExt;
        let env: HashMap<Vec<u8>, Vec<u8>> = std::env::vars_os()
            .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
            .collect();
        let mut trace = VarTrace::default();
        for (name, var) in self.vars.set_vars() {
            let value = var.value.as_ref().unwrap();
            let what = match value {
                Value::Str(s) if var.exported && env.get(name) == Some(s) => What::Environment,
                _ if at_startup => What::Startup,
                _ => What::Before,
            };
            let event = Event {
                what,
                at: None,
                value: self.keeps_history(name).then(|| Some(show_value(value))),
                origin: None,
            };
            trace.vars.entry(name.clone()).or_default().events.push_back(event);
        }
        trace
    }

    fn keeps_history(&self, name: &[u8]) -> bool {
        self.opt(Opt::VarsTraceHistory) || HISTORY_VARS.contains(&name)
    }

    /// Where the code running is, for a change it makes.
    fn location(&self) -> Location {
        let frame = self.frames.last();
        let file = frame.and_then(|f| f.file.clone());
        let place = match file {
            Some(f) => Place::File(f),
            None if self.command_arg.is_some() => Place::Command,
            None if self.interactive => Place::Prompt,
            None => Place::Stdin,
        };
        let lines = frame.is_none_or(|f| f.lines_in_file);
        Location {
            line: if lines { self.lineno } else { 0 },
            function: self.frames.iter().rev().find_map(|f| match &f.kind {
                FrameKind::Function(n) => Some(n.clone()),
                _ => None,
            }),
            place,
        }
    }

    fn record(&mut self, name: &[u8], what: What, origin: Option<Event>) {
        let history = self.keeps_history(name);
        let value = history.then(|| self.vars.get_value(name).map(show_value));
        let event = Event {
            what,
            at: Some(Box::new(self.location())),
            value,
            origin: origin.map(Box::new),
        };
        let Some(trace) = &mut self.vartrace else {
            return;
        };
        let changes = match trace.vars.get_mut(name) {
            Some(c) => c,
            None => trace.vars.entry(name.to_vec()).or_default(),
        };
        if !history {
            changes.events.clear();
            changes.dropped = 0;
        } else if changes.events.len() >= HISTORY_LIMIT {
            changes.events.pop_front();
            changes.dropped += 1;
        }
        changes.events.push_back(event);
    }

    /// Records that `name` was assigned (or changed in place).
    #[cold]
    #[inline(never)]
    pub fn trace_set(&mut self, name: &[u8]) {
        self.record(name, What::Set, None);
    }

    /// Records that `name` was unset.
    #[cold]
    #[inline(never)]
    pub fn trace_unset(&mut self, name: &[u8]) {
        self.record(name, What::Unset, None);
    }

    /// Records that `name` is being put aside (by `local` or a temporary
    /// assignment), for [`Shell::trace_restore`].
    #[cold]
    #[inline(never)]
    pub fn trace_save(&mut self, name: &[u8]) {
        let Some(trace) = &mut self.vartrace else {
            return;
        };
        let last = trace.vars.get(name).and_then(|c| c.events.back().cloned());
        trace.saved.entry(name.to_vec()).or_default().push(last);
    }

    /// Records that the value of `name` that [`Shell::trace_save`] saw was
    /// put back: when `function` returned, or after a command.
    #[cold]
    #[inline(never)]
    pub fn trace_restore(&mut self, name: &[u8], function: Option<&Rc<[u8]>>) {
        let Some(trace) = &mut self.vartrace else {
            return;
        };
        let origin = trace.saved.get_mut(name).and_then(Vec::pop).flatten();
        // Through a chain of restores, the change that set the value.
        let origin = origin.map(|mut o| match (&o.what, o.origin.take()) {
            (What::Restored(_), Some(b)) => *b,
            (_, b) => {
                o.origin = b;
                o
            }
        });
        self.record(name, What::Restored(function.cloned()), origin);
    }

    /// Appends what `where` shows for `name` (with `all`, every change
    /// recorded); false if it isn't set and has no record.
    fn where_one(&self, trace: &VarTrace, p: &Painter, name: &[u8], all: bool, out: &mut Vec<u8>) -> bool {
        let current = self.vars.get_value(name).map(show_value);
        let changes = trace.vars.get(name);
        let Some(last) = changes.and_then(|c| c.events.back()) else {
            let mut t = Text::default();
            match current {
                Some(v) => {
                    t.paint(p, Kind::Name, name);
                    t.plain(b" is set to");
                    let line = Line::new(t, Some(&v), Text::from(b"(where wasn't recorded)"));
                    let wide = line.too_wide(p);
                    line.write(p, wide, out);
                }
                None => {
                    let special = self.vars.special(name).is_some() || name == b"LINENO";
                    t.paint(p, if special { Kind::Name } else { Kind::Unset }, name);
                    t.plain(match special {
                        true => b" is a special parameter, set by the shell",
                        false => b" is not set",
                    });
                    Line::new(t, None, Text::default()).write(p, false, out);
                    return special;
                }
            }
            return true;
        };
        if !all {
            // The value set is the current one, kept or not; for a value put
            // back, the change that set it.
            let shown = match (&last.what, &last.origin) {
                (What::Restored(_), Some(o)) => o,
                _ => last,
            };
            let line = describe(self, p, None, name, shown, Some(current.as_deref()));
            let wide = line.too_wide(p);
            line.write(p, wide, out);
            return true;
        }
        let changes = changes.unwrap();
        if changes.dropped > 0 {
            let n = changes.dropped;
            let s = if n == 1 { "" } else { "s" };
            let mut t = Text::default();
            t.paint(p, Kind::Detail, format!("({n} earlier change{s} of ").as_bytes());
            t.paint(p, Kind::Name, name);
            t.paint(p, Kind::Detail, b" not kept)");
            Line::new(t, None, Text::default()).write(p, false, out);
        }
        let n = changes.events.len();
        let mut lines = Vec::with_capacity(n);
        for (i, e) in changes.events.iter().enumerate() {
            // A value that wasn't kept is the current one, for the last.
            let value = match &e.value {
                Some(v) => Some(v.as_deref()),
                None if i + 1 == n => Some(current.as_deref()),
                None => None,
            };
            let number = changes.dropped + i + 1;
            let label = match i + 1 == n {
                true => format!("[{number} - current state]"),
                false => format!("[{number}]"),
            };
            lines.push(describe(self, p, Some(label.as_bytes()), name, e, value));
        }
        // If one value goes on a line of its own, they all do.
        let wide = lines.iter().any(|l| l.too_wide(p));
        for line in lines {
            line.write(p, wide, out);
        }
        true
    }
}

/// What a piece of what `where` prints is, which picks its style from the
/// line editor's highlighting roles.
#[derive(Clone, Copy)]
enum Kind {
    /// A variable's name.
    Name,
    /// The name of a variable that isn't set.
    Unset,
    /// A value.
    Value,
    /// A file.
    File,
    /// A function's name.
    Function,
    /// The numbers of changes, and notes.
    Detail,
}

const KINDS: [Role; 6] = [
    Role::of("var"),
    Role::of("var.unset"),
    Role::of("string"),
    Role::of("path"),
    Role::of("command.function"),
    Role::of("comment"),
];

/// How `where` prints: its colours (none for plain text), whether files are
/// links, and how wide a line can be before a value goes on a line of its
/// own.
struct Painter {
    sgr: Option<[String; KINDS.len()]>,
    links: bool,
    width: usize,
}

impl Painter {
    fn new(sh: &Shell) -> Painter {
        let cols = crate::sys::isatty(1).then(|| crate::sys::window_size(1)).flatten();
        Painter {
            sgr: crate::builtins::style::stdout_sgr(sh, KINDS),
            links: crate::interactive::links(sh),
            width: cols.map(|c| c.0).filter(|&c| c > 0).unwrap_or(80),
        }
    }

    fn sgr(&self, kind: Kind) -> Option<&str> {
        self.sgr
            .as_ref()
            .map(|s| s[kind as usize].as_str())
            .filter(|s| !s.is_empty())
    }
}

/// Text, painted, with its width on the terminal.
#[derive(Default)]
struct Text {
    s: Vec<u8>,
    width: usize,
}

impl Text {
    fn from(t: &[u8]) -> Text {
        let mut text = Text::default();
        text.plain(t);
        text
    }

    fn plain(&mut self, t: &[u8]) {
        self.s.extend_from_slice(t);
        self.width += width(t);
    }

    fn paint(&mut self, p: &Painter, kind: Kind, t: &[u8]) {
        self.painted(p, kind, t, t);
    }

    /// Appends `t`, which shows as `shown` (without escape sequences), in
    /// the style of `kind`.
    fn painted(&mut self, p: &Painter, kind: Kind, t: &[u8], shown: &[u8]) {
        match p.sgr(kind) {
            Some(on) => {
                self.s.extend_from_slice(on.as_bytes());
                self.s.extend_from_slice(t);
                self.s.extend_from_slice(b"\x1b[m");
            }
            None => self.s.extend_from_slice(t),
        }
        self.width += width(shown);
    }
}

/// The width of `t` on the terminal, counting a character as one column.
fn width(t: &[u8]) -> usize {
    t.iter().filter(|&&c| c & 0xc0 != 0x80).count()
}

/// What `where` prints about a change: `head` (what was done to the
/// variable), the value and `tail` (where), each left out if empty.
struct Line<'a> {
    head: Text,
    value: Option<&'a [u8]>,
    tail: Text,
}

impl<'a> Line<'a> {
    fn new(head: Text, value: Option<&'a [u8]>, tail: Text) -> Line<'a> {
        Line { head, value, tail }
    }

    /// Whether it is wider than the terminal on one line.
    fn too_wide(&self, p: &Painter) -> bool {
        let gap = |t: usize| if t > 0 { t + 1 } else { 0 };
        self.head.width + self.value.map_or(0, |v| gap(width(v))) + gap(self.tail.width) > p.width
    }

    /// Appends it on one line, or with `wide`, with the value and `tail` on
    /// lines of their own below `head`, indented.
    fn write(self, p: &Painter, wide: bool, out: &mut Vec<u8>) {
        let sep: &[u8] = if wide && self.value.is_some() { b"\n    " } else { b" " };
        out.extend(self.head.s);
        if let Some(v) = self.value {
            out.extend_from_slice(sep);
            let mut t = Text::default();
            t.paint(p, Kind::Value, v);
            out.extend(t.s);
        }
        if self.tail.width > 0 {
            out.extend_from_slice(sep);
            out.extend(self.tail.s);
        }
        out.push(b'\n');
    }
}

/// What `e` did to `name`, with `value` the value it set, as shown
/// (`Some(None)` for unset, `None` if not known), after `label`.
fn describe<'a>(
    sh: &Shell,
    p: &Painter,
    label: Option<&[u8]>,
    name: &[u8],
    e: &Event,
    value: Option<Option<&'a [u8]>>,
) -> Line<'a> {
    let mut head = Text::default();
    if let Some(l) = label {
        head.paint(p, Kind::Detail, l);
        head.plain(b" ");
    }
    head.paint(p, Kind::Name, name);
    let unset = value == Some(None);
    let value = value.flatten();
    let mut tail = Text::default();
    let to = |head: &mut Text, prefix: &[u8]| {
        if value.is_some() {
            head.plain(prefix);
        }
    };
    match &e.what {
        What::Set => {
            head.plain(b" was set");
            to(&mut head, b" to");
        }
        What::Unset => head.plain(b" was unset"),
        What::Restored(_) if unset => head.plain(b" was unset again"),
        What::Restored(_) => {
            head.plain(b" was restored");
            to(&mut head, b" to");
        }
        What::Environment => {
            head.plain(b" was inherited from the environment");
            to(&mut head, b" as");
        }
        What::Startup => {
            head.plain(b" was set");
            to(&mut head, b" to");
            tail.plain(b"by luish when it started");
        }
        What::Before => {
            head.plain(b" was set");
            to(&mut head, b" to");
            tail.plain(b"before tracing began");
        }
    }
    match (&e.what, &e.at) {
        (What::Restored(Some(f)), _) => {
            tail.plain(b"on return from function ");
            tail.paint(p, Kind::Function, f);
        }
        (What::Restored(None), Some(at)) => {
            tail.plain(b"after the command ");
            location(sh, p, at, &mut tail);
        }
        (_, Some(at)) => location(sh, p, at, &mut tail),
        (_, None) => {}
    }
    let value = match &e.what {
        What::Unset => None,
        _ => value,
    };
    Line::new(head, value, tail)
}

/// Appends where `at` is: `in FILE:LINE`, `at the prompt`, ..., and the
/// function running.
fn location(sh: &Shell, p: &Painter, at: &Location, out: &mut Text) {
    let line = (at.line > 0).then(|| at.line.to_string());
    match &at.place {
        Place::File(f) => {
            out.plain(b"in ");
            let path = f.display_path();
            let mut shown = tilde(sh, &path);
            if let Some(l) = &line {
                shown.push(b':');
                shown.extend_from_slice(l.as_bytes());
            }
            match p.links && path.first() == Some(&b'/') {
                true => out.painted(p, Kind::File, &crate::interactive::file_link(&shown, &path), &shown),
                false => out.paint(p, Kind::File, &shown),
            }
        }
        Place::Prompt => out.plain(b"at the prompt"),
        Place::Command => {
            out.plain(b"in the -c command");
            if let Some(l) = &line {
                out.plain(format!(", line {l}").as_bytes());
            }
        }
        Place::Stdin => {
            out.plain(b"on standard input");
            if let Some(l) = &line {
                out.plain(format!(", line {l}").as_bytes());
            }
        }
    }
    if let Some(f) = &at.function {
        out.plain(b", in function ");
        out.paint(p, Kind::Function, f);
    }
}

/// `path` with `$HOME` at its start written `~`.
fn tilde(sh: &Shell, path: &[u8]) -> Vec<u8> {
    if let Some(home) = sh.vars.get(b"HOME").filter(|h| h.len() > 1)
        && let Some(rest) = path.strip_prefix(home)
        && (rest.is_empty() || rest[0] == b'/')
    {
        return [b"~", rest].concat();
    }
    path.to_vec()
}

/// A value as `where` shows it: a string in double quotes (or as `$'...'`
/// if it has control characters), an array as `("a" "b")` and an
/// associative array as `([k]="v")`, cut after [`MAX_VALUE`] bytes.
fn show_value(v: &Value) -> Box<[u8]> {
    let mut out = Vec::new();
    match v {
        Value::Str(s) => quote(s, &mut out),
        Value::Array(a) => {
            out.push(b'(');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                quote(e, &mut out);
            }
            out.push(b')');
        }
        Value::Assoc(h) => {
            out.push(b'(');
            for (i, (k, e)) in h.keys().iter().zip(h.values()).enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.push(b'[');
                quote(k, &mut out);
                out.extend_from_slice(b"]=");
                quote(e, &mut out);
            }
            out.push(b')');
        }
    }
    if out.len() > MAX_VALUE {
        out.truncate(MAX_VALUE);
        // Not in the middle of a UTF-8 sequence.
        while out.last().is_some_and(|&c| c & 0xc0 == 0x80) {
            out.pop();
        }
        out.pop_if(|c| *c >= 0xc0);
        out.extend_from_slice(b"...");
    }
    out.into_boxed_slice()
}

/// Appends `s` quoted for the shell: in double quotes, or as `$'...'` if
/// it has control characters (which the terminal would act on).
fn quote(s: &[u8], out: &mut Vec<u8>) {
    if !s.iter().any(|&c| c < b' ' || c == 0x7f) {
        out.push(b'"');
        for &c in s {
            if matches!(c, b'"' | b'\\' | b'$' | b'`') {
                out.push(b'\\');
            }
            out.push(c);
        }
        out.push(b'"');
        return;
    }
    out.extend_from_slice(b"$'");
    for &c in s {
        match c {
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0x1b => out.extend_from_slice(b"\\e"),
            b'\'' | b'\\' => out.extend_from_slice(&[b'\\', c]),
            c if c < b' ' || c == 0x7f => out.extend_from_slice(format!("\\x{c:02x}").as_bytes()),
            c => out.push(c),
        }
    }
    out.push(b'\'');
}

/// `where [-a] [name...]`: where each variable was last set (all of them
/// without names), or with `-a`, the changes recorded.
pub fn where_(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    let (opts, args) = match crate::builtins::options(sh, argv, b"a") {
        Ok(r) => r,
        Err(s) => return Ok(s),
    };
    let Some(trace) = sh.vartrace.as_deref() else {
        sh.berr(
            &argv[0],
            "variable tracing is off (turn it on with `setopt vars.trace`)",
        );
        return Ok(2);
    };
    let all = !opts.is_empty();
    let names: Vec<Vec<u8>> = match args {
        [] => sh
            .vars
            .sorted()
            .into_iter()
            .filter(|(_, v)| v.value.is_some())
            .map(|(k, _)| k.clone())
            .collect(),
        _ => args.to_vec(),
    };
    let p = Painter::new(sh);
    let mut out = Vec::new();
    let mut status = 0;
    for (i, name) in names.iter().enumerate() {
        // With -a, a blank line between variables.
        if all && i > 0 {
            out.push(b'\n');
        }
        if !crate::lexer::is_valid_name(name) {
            sh.out(&out);
            out.clear();
            sh.berr(
                &argv[0],
                format!("{}: bad variable name", String::from_utf8_lossy(name)),
            );
            status = 2;
            continue;
        }
        // `path` is `PATH` while it is tied to it.
        let name: &[u8] = match sh.vars.special(name) {
            Some(Special::Path) => b"PATH",
            _ => name,
        };
        if !sh.where_one(trace, &p, name, all, &mut out) && status == 0 {
            status = 1;
        }
    }
    sh.out(&out);
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painter(sgr: bool) -> Painter {
        Painter {
            sgr: sgr.then(|| KINDS.map(|r| format!("\x1b[{}m", r.index().unwrap()))),
            links: false,
            width: 30,
        }
    }

    fn line(p: &Painter, value: &'static [u8]) -> Line<'static> {
        let mut head = Text::default();
        head.paint(p, Kind::Name, b"X");
        head.plain(b" was set to");
        let mut tail = Text::from(b"in function ");
        tail.paint(p, Kind::Function, b"f");
        Line::new(head, Some(value), tail)
    }

    fn written(p: &Painter, value: &'static [u8]) -> String {
        let l = line(p, value);
        let mut out = Vec::new();
        let wide = l.too_wide(p);
        l.write(p, wide, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn plain_layout() {
        let p = painter(false);
        assert_eq!(written(&p, b"\"1\""), "X was set to \"1\" in function f\n");
        assert_eq!(
            written(&p, b"\"12345678\""),
            "X was set to\n    \"12345678\"\n    in function f\n"
        );
    }

    #[test]
    fn width_leaves_out_escapes() {
        // The same layout with colours.
        let p = painter(true);
        let [name, value, function] =
            [Kind::Name, Kind::Value, Kind::Function].map(|k| KINDS[k as usize].index().unwrap());
        assert_eq!(
            written(&p, b"\"1\""),
            format!("\x1b[{name}mX\x1b[m was set to \x1b[{value}m\"1\"\x1b[m in function \x1b[{function}mf\x1b[m\n")
        );
        assert!(written(&p, b"\"12345678\"").contains("\n    "));
        // Characters, not bytes.
        assert_eq!(width("é".as_bytes()), 1);
    }
}
