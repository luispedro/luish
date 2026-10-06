//! The line editor's side of the SSH mode (`crate::remote`). On the server,
//! `serve_line` sends the client what the editor needs to read a line (a
//! [`Request`]) and answers its questions (Tab, and files for the
//! highlighter) until the line comes back. On the client, `client_line`
//! reads the line as the request asks, with the editor's questions sent
//! back to the server through a [`Link`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::complete::{Names, ShellHelper, Tab};
use super::highlight::{Colors, Found, Query, VarKind};
use super::history::{Changes, Sent};
use super::menu::Item;
use super::{EDITOR, EXIT, Request, SHELL};
use crate::hash::HashMap;
use crate::input::Line;
use crate::lexer::AliasMap;
use crate::remote::{Dec, Enc, LineReply, Reader, msg};
use crate::shell::Shell;
use crate::style::{Color, Style};
use crate::sys;

/// What the shell writes to the pty before it asks for a line, so that the
/// relay sends the client all the output before the request (`relay.rs`).
pub const SYNC: &[u8] = b"\x1b]6973;luish-sync\x07";

/// How long the highlighter waits for the server to answer about a file
/// before it draws the line without the answer.
const LOOKUP_WAIT: Duration = Duration::from_millis(20);

/// The highlighter's questions about files, with their answers (None
/// until they come).
type Lookups = HashMap<(Query, Vec<u8>), Option<Found>>;

thread_local! {
    /// On the server: the shell's end of the socket to the relay.
    static SERVE: Cell<i32> = const { Cell::new(-1) };
    static FROM_RELAY: RefCell<Reader> = RefCell::default();
    /// What the client has been sent of the history.
    static SENT: RefCell<Sent> = RefCell::default();
    /// The generation of the commands on `PATH` that the client has.
    static COMMANDS_SENT: Cell<u64> = const { Cell::new(0) };

    /// On the client: the connection to the server.
    static LINK: RefCell<Option<Box<dyn Link>>> = const { RefCell::new(None) };
    /// The commands on the server's `PATH`.
    static COMMANDS: RefCell<Rc<Vec<Vec<u8>>>> = RefCell::default();
    /// The highlighter's questions since the prompt.
    static LOOKUPS: RefCell<Lookups> = RefCell::default();
    /// Counts the requests, so that answers about the files for an earlier
    /// one are dropped.
    static PROMPT: Cell<u64> = const { Cell::new(0) };
    /// Keys typed while the last command ran that the next lines start
    /// with.
    static AHEAD: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Whether this shell is the server of the SSH mode.
pub fn serving() -> bool {
    SERVE.get() >= 0
}

/// Makes this shell the server, talking to the relay over `fd`.
pub fn start_serving(fd: i32) {
    SERVE.set(fd);
}

/// Reads a line through the client, as `req` asks, answering its questions
/// meanwhile. The shell exits if the relay has gone.
pub fn serve_line(sh: &mut Shell, req: Request) -> Line {
    let fd = SERVE.get();
    let history = super::with_history(|h| SENT.with_borrow_mut(|s| h.changes(s)));
    // The completer runs here, with the names of this prompt.
    let commands = EDITOR.with_borrow_mut(|e| {
        let h = e.as_mut()?.helper_mut()?;
        h.names = req.names.clone();
        let (generation, names) = h.path_commands(COMMANDS_SENT.get());
        COMMANDS_SENT.set(generation);
        names
    });
    // Once the relay holds the keys that come, those on the pty are all
    // there is to take.
    if !crate::remote::write_frame(fd, msg::HOLD, &[]) {
        sh.exit(128 + libc::SIGHUP);
    }
    loop {
        match FROM_RELAY.with_borrow_mut(|r| r.read(fd)) {
            Some(f) if f.kind == msg::HELD => break,
            Some(_) => {}
            None => sh.exit(128 + libc::SIGHUP),
        }
    }
    let mut e = Enc::default();
    encode_request(&mut e, &req, history.as_ref(), commands.as_deref(), &typeahead());
    sys::write_all(1, SYNC);
    if !crate::remote::write_frame(fd, msg::REQUEST, &e.buf) {
        sh.exit(128 + libc::SIGHUP);
    }
    SHELL.set(sh as *mut Shell);
    let line = loop {
        let Some(f) = FROM_RELAY.with_borrow_mut(|r| r.read(fd)) else {
            break None;
        };
        match f.kind {
            msg::LINE => match LineReply::decode(&f.data) {
                Some(LineReply::Text(t)) => break Some(Line::Text(t)),
                Some(LineReply::Interrupted) => break Some(Line::Interrupted),
                _ => break Some(Line::Eof),
            },
            msg::TAB => {
                let mut d = Dec::new(&f.data);
                let (Some(before), Some(after)) = (d.bytes(), d.bytes()) else {
                    continue;
                };
                let tab = EDITOR.with_borrow(|e| Some(e.as_ref()?.helper()?.tab(&before, &after)));
                let mut e = Enc::default();
                encode_tab(&mut e, &tab.unwrap_or(Tab::Matches(before.len(), Vec::new())));
                crate::remote::write_frame(fd, msg::TAB_REPLY, &e.buf);
            }
            msg::LOOKUP => {
                let mut d = Dec::new(&f.data);
                let (Some(prompt), Some(q), Some(path)) = (d.u64(), d.u8().and_then(query), d.bytes()) else {
                    continue;
                };
                let mut e = Enc::default();
                e.u64(prompt).u8(q as u8).bytes(&path);
                encode_found(&mut e, &super::highlight::look_up(q, &path));
                crate::remote::write_frame(fd, msg::LOOKUP_REPLY, &e.buf);
            }
            _ => {}
        }
    };
    SHELL.set(std::ptr::null_mut());
    if let Some(n) = EXIT.take() {
        sh.exit(n);
    }
    line.unwrap_or_else(|| sh.exit(128 + libc::SIGHUP))
}

/// The keys typed on the pty that no command read, which the client starts
/// the next line with.
fn typeahead() -> Vec<u8> {
    let Some(saved) = sys::tcgetattr(0) else {
        return Vec::new();
    };
    let mut t = saved;
    t.c_lflag &= !(libc::ICANON | libc::ECHO);
    t.c_cc[libc::VMIN] = 0;
    t.c_cc[libc::VTIME] = 0;
    sys::tcsetattr(0, &t);
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = sys::read(0, &mut buf, false) {
        out.extend_from_slice(&buf[..n]);
    }
    sys::tcsetattr(0, &saved);
    out
}

/// The client's connection to the server (see `remote/client.rs`).
pub trait Link {
    /// Sends a message to the shell. False if the connection has gone.
    fn send(&self, kind: u8, data: &[u8]) -> bool;
    /// The next message from the shell, waiting at most `wait` (or until
    /// one comes, if None). None if none came, or the connection has gone.
    fn reply(&self, wait: Option<Duration>) -> Option<(u8, Vec<u8>)>;
}

/// Sets up the client's line editor, which asks `link` what Tab does and
/// about files. Returns false if the editor can't be used.
pub fn init_client(link: Box<dyn Link>) -> bool {
    LINK.set(Some(link));
    let mut helper = ShellHelper::default();
    helper.remote_tab = Some(remote_tab);
    helper.lookup = Some(lookup);
    super::install_editor(helper)
}

/// Reads a line as the request in `data` asks, after the relay's count.
/// `unsent` gives the keys sent to the server from that count on, which
/// the pty didn't get. None if the request can't be read (from a server
/// that speaks another protocol).
pub fn client_line(data: &[u8], unsent: impl FnOnce(u64) -> Vec<u8>) -> Option<LineReply> {
    let mut d = Dec::new(data);
    let held_at = d.u64()?;
    let mut req = decode_request(&mut d)?;
    let changes = decode_changes(&mut d)?;
    let commands = match d.opt_bytes()? {
        Some(b) => Some(decode_list(&b)?),
        None => None,
    };
    // Those the pty got first, then the others.
    let mut typed = d.bytes()?;
    typed.extend(unsent(held_at));
    super::with_history(|h| h.apply(changes));
    if let Some(c) = commands {
        COMMANDS.set(Rc::new(c));
    }
    req.names.commands = Some(COMMANDS.with_borrow(Rc::clone));
    PROMPT.set(PROMPT.get() + 1);
    LOOKUPS.with_borrow_mut(HashMap::clear);
    // Keys typed ahead: a whole line runs as it is, without editing; the
    // rest starts the line.
    let ahead = AHEAD.with_borrow_mut(|a| {
        a.extend(typed.iter().filter(|&&c| c >= b' ' || c == b'\t' || c == b'\n'));
        match a.iter().position(|&c| c == b'\n') {
            Some(i) => Err(a.drain(..=i).collect::<Vec<u8>>()),
            None => Ok(std::mem::take(a)),
        }
    });
    match ahead {
        Err(line) => {
            sys::write_all(1, req.prompt.as_bytes());
            sys::write_all(1, &line);
            return Some(LineReply::Text(line));
        }
        Ok(text) if !text.is_empty() => {
            let mut start = req.start.take().unwrap_or_default();
            start.push_str(&String::from_utf8_lossy(&text));
            req.start = Some(start);
        }
        Ok(_) => {}
    }
    Some(match super::edit(req) {
        Line::Text(t) => LineReply::Text(t),
        Line::Interrupted => LineReply::Interrupted,
        Line::Eof => LineReply::Eof,
    })
}

/// Tab, in the client: the server works out what it does.
fn remote_tab(before: &[u8], after: &[u8]) -> Tab {
    let none = || Tab::Matches(before.len(), Vec::new());
    LINK.with_borrow(|link| {
        let Some(link) = link else { return none() };
        let mut e = Enc::default();
        e.bytes(before).bytes(after);
        if !link.send(msg::TAB, &e.buf) {
            return none();
        }
        loop {
            match link.reply(None) {
                Some((msg::TAB_REPLY, data)) => return decode_tab(&mut Dec::new(&data)).unwrap_or_else(none),
                Some((msg::LOOKUP_REPLY, data)) => store_lookup(&data),
                Some(_) => {}
                None => return none(),
            }
        }
    })
}

/// A question about a file, in the client: the server answers, if it does
/// so soon enough; a later answer is used at a later redraw.
fn lookup(q: Query, path: &[u8]) -> Option<Found> {
    LINK.with_borrow(|link| {
        let link = link.as_ref()?;
        // The answers that came since.
        while let Some((kind, data)) = link.reply(Some(Duration::ZERO)) {
            if kind == msg::LOOKUP_REPLY {
                store_lookup(&data);
            }
        }
        let key = (q, path.to_vec());
        if let Some(found) = LOOKUPS.with_borrow(|l| l.get(&key).cloned()) {
            return found;
        }
        let mut e = Enc::default();
        e.u64(PROMPT.get()).u8(q as u8).bytes(path);
        if !link.send(msg::LOOKUP, &e.buf) {
            return None;
        }
        LOOKUPS.with_borrow_mut(|l| l.insert(key.clone(), None));
        let until = Instant::now() + LOOKUP_WAIT;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            let (kind, data) = link.reply(Some(left))?;
            if kind == msg::LOOKUP_REPLY {
                store_lookup(&data);
                if let Some(Some(found)) = LOOKUPS.with_borrow(|l| l.get(&key).cloned()) {
                    return Some(found);
                }
            }
        }
    })
}

/// Keeps the server's answer about a file, unless it was for an earlier
/// prompt.
fn store_lookup(data: &[u8]) {
    let mut d = Dec::new(data);
    let (Some(prompt), Some(q), Some(path), Some(found)) =
        (d.u64(), d.u8().and_then(query), d.bytes(), decode_found(&mut d))
    else {
        return;
    };
    if prompt == PROMPT.get() {
        LOOKUPS.with_borrow_mut(|l| l.insert((q, path), Some(found)));
    }
}

fn query(n: u8) -> Option<Query> {
    [Query::Executable, Query::Dir, Query::Exists, Query::List]
        .get(n as usize)
        .copied()
}

fn encode_found(e: &mut Enc, f: &Found) {
    match f {
        Found::Bool(b) => e.u8(0).bool(*b),
        Found::Names(None) => e.u8(1),
        Found::Names(Some(names)) => e.u8(2).list(names, |e, n| {
            e.bytes(n);
        }),
    };
}

fn decode_found(d: &mut Dec) -> Option<Found> {
    Some(match d.u8()? {
        0 => Found::Bool(d.bool()?),
        1 => Found::Names(None),
        _ => Found::Names(Some(d.list(Dec::bytes)?)),
    })
}

fn encode_tab(e: &mut Enc, tab: &Tab) {
    match tab {
        Tab::Expand(start, text) => {
            e.u8(0).usize(*start).str(text);
        }
        Tab::Matches(start, items) => {
            e.u8(1).usize(*start).list(items, |e, i| {
                e.str(&i.display).opt_str(i.desc.as_deref()).str(&i.replacement);
            });
        }
    }
}

fn decode_tab(d: &mut Dec) -> Option<Tab> {
    Some(match d.u8()? {
        0 => Tab::Expand(d.usize()?, d.string()?),
        _ => {
            let start = d.usize()?;
            let items = d.list(|d| {
                Some(Item {
                    display: d.string()?,
                    desc: d.opt_string()?,
                    replacement: d.string()?,
                })
            })?;
            Tab::Matches(start, items)
        }
    })
}

fn encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let mut e = Enc::default();
    e.list(items, |e, i| {
        e.bytes(i);
    });
    e.buf
}

fn decode_list(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    Dec::new(data).list(Dec::bytes)
}

/// The request, then the changes to the history, the commands on `PATH`
/// if they changed, and the keys typed ahead.
fn encode_request(e: &mut Enc, req: &Request, history: Option<&Changes>, commands: Option<&[Vec<u8>]>, typed: &[u8]) {
    e.bool(req.continuation)
        .bytes(&req.pending)
        .str(&req.prompt)
        .opt_str(req.plain.as_deref());
    match &req.right {
        Some(p) => e.u8(1).bytes(&p.text).opt_bytes(p.plain.as_deref()),
        None => e.u8(0),
    };
    e.usize(req.indent)
        .bool(req.transient)
        .bool(req.vi)
        .bool(req.suggest)
        .bool(req.marks)
        .bool(req.highlight_on);
    encode_names(e, &req.names);
    let roles: Vec<_> = req.colors.roles().collect();
    e.list(&roles, |e, (name, style)| {
        e.str(name);
        encode_style(e, style);
    });
    e.opt_str(req.wordchars.as_deref());
    let (version, changes) = req.keymap.to_wire();
    e.u64(version).list(&changes, |e, (seq, w)| {
        e.str(seq).opt_str(*w);
    });
    e.opt_str(req.start.as_deref());
    match history {
        Some(h) => e.usize(h.first).bool(h.reset).list(&h.added, |e, t| {
            e.str(t);
        }),
        None => e.usize(1).bool(false).u32(0),
    };
    e.opt_bytes(commands.map(encode_list).as_deref()).bytes(typed);
}

fn decode_request(d: &mut Dec) -> Option<Request> {
    let continuation = d.bool()?;
    let pending = d.bytes()?;
    let prompt = d.string()?;
    let plain = d.opt_string()?;
    let right = match d.u8()? {
        0 => None,
        _ => Some(crate::prompt::Prompt {
            text: d.bytes()?,
            plain: d.opt_bytes()?,
        }),
    };
    let indent = d.usize()?;
    let (transient, vi, suggest, marks, highlight_on) = (d.bool()?, d.bool()?, d.bool()?, d.bool()?, d.bool()?);
    let names = decode_names(d)?;
    let roles: HashMap<String, Style> = d.list(|d| Some((d.string()?, decode_style(d)?)))?.into_iter().collect();
    let colors = Rc::new(Colors::new(|n| roles.get(n).cloned().unwrap_or_default()));
    let wordchars = d.opt_string()?;
    let version = d.u64()?;
    let changes = d.list(|d| Some((d.string()?, d.opt_string()?)))?;
    let keymap = super::keys::Keymap::from_wire(version, changes);
    let start = d.opt_string()?;
    Some(Request {
        continuation,
        pending,
        prompt,
        plain,
        right,
        indent,
        transient,
        vi,
        suggest,
        marks,
        names,
        colors,
        highlight_on,
        wordchars,
        keymap,
        start,
    })
}

fn decode_changes(d: &mut Dec) -> Option<Changes> {
    Some(Changes {
        first: d.usize()?,
        reset: d.bool()?,
        added: d.list(Dec::string)?,
    })
}

/// What the highlighter uses of the names; completion runs on the server.
fn encode_names(e: &mut Enc, n: &Names) {
    e.list(&n.functions, |e, f| {
        e.bytes(f);
    });
    e.list(&n.aliases.sorted(), |e, (name, a)| {
        e.bytes(name).bytes(&a.value).bool(a.global);
    });
    e.list(&n.aliases.sorted_suffixes(), |e, (suffix, value)| {
        e.bytes(suffix).bytes(value);
    });
    let mut vars: Vec<_> = n.vars.iter().collect();
    vars.sort_unstable_by_key(|v| v.0);
    e.list(&vars, |e, (name, kind)| {
        let k = [VarKind::Plain, VarKind::Readonly, VarKind::Array, VarKind::Exported]
            .iter()
            .position(|k| k == *kind)
            .unwrap_or(0);
        e.bytes(name).u8(k as u8);
    });
    e.bytes(&n.path).opt_bytes(n.home.as_deref()).bytes(&n.cdpath);
    e.list(&n.builtins, |e, b| {
        e.bytes(b);
    });
    e.bool(n.autocd)
        .bool(n.braces)
        .bool(n.glob)
        .bool(n.bareglobqual)
        .bool(n.paths)
        .bool(n.history_expand);
}

fn decode_names(d: &mut Dec) -> Option<Names> {
    let functions = d.list(Dec::bytes)?;
    let mut aliases = AliasMap::default();
    for (name, value, global) in d.list(|d| Some((d.bytes()?, d.bytes()?, d.bool()?)))? {
        aliases.insert(name, value, global);
    }
    for (suffix, value) in d.list(|d| Some((d.bytes()?, d.bytes()?)))? {
        aliases.insert_suffix(suffix, value);
    }
    let kinds = [VarKind::Plain, VarKind::Readonly, VarKind::Array, VarKind::Exported];
    let vars = d.list(|d| Some((d.bytes()?, *kinds.get(d.u8()? as usize)?)))?;
    Some(Names {
        functions,
        aliases: Rc::new(aliases),
        vars: vars.into_iter().collect(),
        path: d.bytes()?,
        home: d.opt_bytes()?,
        cdpath: d.bytes()?,
        builtins: d.list(Dec::bytes)?,
        autocd: d.bool()?,
        braces: d.bool()?,
        glob: d.bool()?,
        bareglobqual: d.bool()?,
        paths: d.bool()?,
        history_expand: d.bool()?,
        ..Names::default()
    })
}

fn encode_color(e: &mut Enc, c: Option<Color>) {
    match c {
        None => e.u8(0),
        Some(Color::Default) => e.u8(1),
        Some(Color::Index(i)) => e.u8(2).u8(i),
        Some(Color::Rgb(r, g, b)) => e.u8(3).u8(r).u8(g).u8(b),
    };
}

fn decode_color(d: &mut Dec) -> Option<Option<Color>> {
    Some(match d.u8()? {
        0 => None,
        1 => Some(Color::Default),
        2 => Some(Color::Index(d.u8()?)),
        _ => Some(Color::Rgb(d.u8()?, d.u8()?, d.u8()?)),
    })
}

fn encode_style(e: &mut Enc, s: &Style) {
    encode_color(e, s.fg);
    encode_color(e, s.bg);
    e.u8(s.on).u8(s.off);
    match s.under {
        Some(u) => e.u8(1).u8(u),
        None => e.u8(0),
    };
    encode_color(e, s.ul);
    e.bool(s.plain).opt_str(s.raw.as_deref());
}

fn decode_style(d: &mut Dec) -> Option<Style> {
    let fg = decode_color(d)?;
    let bg = decode_color(d)?;
    let (on, off) = (d.u8()?, d.u8()?);
    let under = match d.u8()? {
        0 => None,
        _ => Some(d.u8()?),
    };
    Some(Style {
        fg,
        bg,
        on,
        off,
        under,
        ul: decode_color(d)?,
        plain: d.bool()?,
        raw: d.opt_string()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trip() {
        let mut aliases = AliasMap::default();
        aliases.insert(b"ll".to_vec(), b"ls -l".to_vec(), false);
        aliases.insert_suffix(b"txt".to_vec(), b"less".to_vec());
        let names = Names {
            functions: vec![b"f".to_vec()],
            aliases: Rc::new(aliases),
            vars: [(b"PATH".to_vec(), VarKind::Exported)].into_iter().collect(),
            home: Some(b"/home/u".to_vec()),
            autocd: true,
            ..Names::default()
        };
        let styles = crate::style::Styles::default();
        let r = styles.resolver(Some("default-dark"));
        let req = Request {
            continuation: false,
            pending: b"if x\n".to_vec(),
            prompt: "$ ".into(),
            plain: None,
            right: Some(crate::prompt::Prompt::plain(b"[r]".to_vec())),
            indent: 1,
            transient: true,
            vi: false,
            suggest: true,
            marks: false,
            names,
            colors: Rc::new(Colors::new(|n| r.get(n))),
            highlight_on: true,
            wordchars: Some("*?".into()),
            keymap: Default::default(),
            start: Some("echo".into()),
        };
        let changes = Changes {
            first: 3,
            reset: true,
            added: vec!["ls".into()],
        };
        let mut e = Enc::default();
        encode_request(&mut e, &req, Some(&changes), Some(&[b"ls".to_vec()]), b"x");
        let mut d = Dec::new(&e.buf);
        let got = decode_request(&mut d).unwrap();
        assert_eq!(got.pending, req.pending);
        assert_eq!(got.right.unwrap().text, b"[r]");
        assert!(got.transient && got.suggest && !got.vi);
        assert_eq!(got.names.aliases.get(b"ll").unwrap().value, b"ls -l");
        assert_eq!(got.names.aliases.sorted_suffixes().len(), 1);
        assert_eq!(got.names.vars.get(&b"PATH"[..]), Some(&VarKind::Exported));
        assert_eq!(got.names.home.as_deref(), Some(&b"/home/u"[..]));
        assert!(got.names.autocd);
        let role = super::super::highlight::role::UNKNOWN;
        assert_eq!(got.colors.sgr(role), req.colors.sgr(role));
        assert_eq!(got.start.as_deref(), Some("echo"));
        let c = decode_changes(&mut d).unwrap();
        assert_eq!((c.first, c.reset, c.added), (3, true, vec!["ls".to_string()]));
        assert_eq!(decode_list(&d.opt_bytes().unwrap().unwrap()).unwrap(), [b"ls".to_vec()]);
        assert_eq!(d.bytes().unwrap(), b"x");
    }

    #[test]
    fn tab_round_trip() {
        let item = Item {
            display: "file".into(),
            desc: Some("d".into()),
            replacement: "dir/file".into(),
        };
        let mut e = Enc::default();
        encode_tab(&mut e, &Tab::Matches(4, vec![item.clone()]));
        match decode_tab(&mut Dec::new(&e.buf)) {
            Some(Tab::Matches(4, items)) => assert_eq!(items, [item]),
            _ => panic!(),
        }
        let mut e = Enc::default();
        encode_found(&mut e, &Found::Names(Some(vec![b"a".to_vec()])));
        assert_eq!(
            decode_found(&mut Dec::new(&e.buf)),
            Some(Found::Names(Some(vec![b"a".to_vec()])))
        );
    }
}
