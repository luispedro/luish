//! Interactive mode: prompts, the line editor, history, and startup files.

pub(crate) mod bang;
mod complete;
pub mod firstrun;
pub(crate) mod highlight;
mod histfile;
pub mod history;
pub mod keys;
mod menu;
mod rprompt;

use std::cell::{Cell, RefCell};

use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::history::History;
use rustyline::{CompletionType, Config, Editor};

pub use complete::Completion;
#[cfg(feature = "plugins")]
pub use complete::{Candidate, DEFAULT_COMPLETER, Suffix};
use complete::{Names, ShellHelper};
use history::{Save, ShellHistory};

use crate::input::Line;
use crate::jobs::JobTable;
use crate::options::Opt;
use crate::shell::{Flow, Shell};
use crate::sys;
use crate::vars::Value;

thread_local! {
    static EDITOR: RefCell<Option<Editor<ShellHelper, ShellHistory>>> = const { RefCell::new(None) };
    /// The shell, while the editor reads a line (for `ask`).
    static SHELL: Cell<*mut Shell> = const { Cell::new(std::ptr::null_mut()) };
    /// Set when a completer exits the shell, which happens once the editor
    /// has given the terminal back.
    static EXIT: Cell<Option<i32>> = const { Cell::new(None) };
    /// The event number of the history entry to start the next command
    /// line with (after `accept-line-and-down-history`).
    static NEXT: Cell<Option<usize>> = const { Cell::new(None) };
    /// The text to start the next line with: a line whose history
    /// references were expanded, with `history.verify`.
    static REFILL: RefCell<Option<String>> = const { RefCell::new(None) };
    /// zsh's buffer stack: text that `print -z` pushed, the last of which
    /// starts the next command line.
    static PUSHED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// What history expansion keeps from one line to the next.
    static BANG: RefCell<bang::Memory> = RefCell::default();
    /// Whether the editor can start a line with text: not on the terminals
    /// rustyline doesn't support, where it reads lines without editing.
    static EDITS: Cell<bool> = const { Cell::new(true) };
}

/// `$HISTFILE`, or by default `$XDG_STATE_HOME/luish/history` (or
/// `~/.local/state/luish/history`). An empty `HISTFILE` means none.
fn history_file(sh: &Shell) -> Option<Vec<u8>> {
    match sh.get_var(b"HISTFILE") {
        Some(h) => Some(h).filter(|h| !h.is_empty()),
        None => {
            let mut d = crate::startcache::xdg_dir(sh, b"XDG_STATE_HOME", b"/.local/state")?;
            d.extend_from_slice(b"/luish/history");
            Some(d)
        }
    }
}

fn number_var(sh: &Shell, name: &[u8]) -> Option<usize> {
    String::from_utf8(sh.get_var(name)?).ok()?.parse().ok()
}

/// The number of entries kept in memory: `$HISTSIZE`, 1000 by default.
fn history_size(sh: &Shell) -> usize {
    number_var(sh, b"HISTSIZE").unwrap_or(1000)
}

/// How the history is saved to `path`: `$SAVEHIST` entries are kept (by
/// default as many as `$HISTSIZE`).
fn save_options<'a>(sh: &Shell, path: &'a [u8]) -> Save<'a> {
    Save {
        path,
        limit: number_var(sh, b"SAVEHIST").unwrap_or_else(|| history_size(sh)),
        share: sh.opt(Opt::ShareHistory),
        no_dups: sh.opt(Opt::HistSaveNoDups),
    }
}

pub fn to_path(b: &[u8]) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b).into()
}

/// Sets up the line editor. Returns false if it can't be used.
pub fn init_editor() -> bool {
    let Ok(mut ed) = Editor::with_history(Config::default(), ShellHistory::default()) else {
        return false;
    };
    let mut helper = ShellHelper::default();
    helper.ask = Some(ask);
    helper.expand = Some(expand);
    helper.subscripts = Some(subscripts);
    ed.history_mut().search = helper.keys.lock().map(|k| k.search.clone()).unwrap_or_default();
    keys::bind(&mut ed, &helper.menu, &helper.keys);
    ed.set_helper(Some(helper));
    ed.set_completion_type(CompletionType::List);
    // As rustyline's `is_unsupported_term`.
    let term = std::env::var("TERM").unwrap_or_default();
    EDITS.set(
        !["dumb", "cons25", "emacs"]
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&term)),
    );
    EDITOR.with(|e| *e.borrow_mut() = Some(ed));
    true
}

/// Reads the history file, once the startup files have set `HISTFILE`
/// and `HISTSIZE`.
pub fn load_history(sh: &Shell) {
    let size = history_size(sh);
    let file = history_file(sh);
    with_history(|h| {
        let _ = h.set_max_len(size);
        if let Some(f) = file {
            h.load(&f);
        }
    });
}

/// Appends the new entries to the history file (on exit, or after each
/// command with `history.inc_append` or `history.share`).
pub fn save_history(sh: &Shell) {
    let Some(f) = history_file(sh) else { return };
    let save = save_options(sh, &f);
    with_history(|h| h.save(&save));
}

/// Adds the text of a command about to be run to the history.
pub fn add_history(sh: &Shell, text: &[u8]) {
    let text = String::from_utf8_lossy(text);
    let text = text.trim_end_matches('\n');
    if text.trim().is_empty() {
        return;
    }
    let private = sh.opt(Opt::HistIgnoreSpace) && text.starts_with([' ', '\t']);
    let reduced;
    let text = if sh.opt(Opt::HistReduceBlanks) {
        reduced = history::reduce_blanks(text);
        &reduced
    } else {
        text
    };
    with_history(|h| h.add_current(text, private));
    if sh.opt(Opt::IncAppendHistory) || sh.opt(Opt::ShareHistory) {
        save_history(sh);
    }
}

/// Adds `text` to the history as an entry of its own, after the command
/// being run (`print -s`).
pub fn add_history_entry(sh: &Shell, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    with_history(|h| h.add_entry(text));
    if sh.opt(Opt::IncAppendHistory) || sh.opt(Opt::ShareHistory) {
        save_history(sh);
    }
}

/// Pushes text onto the buffer stack, to start a command line with
/// (`print -z`).
pub fn push_buffer(text: String) {
    PUSHED.with_borrow_mut(|p| p.push(text));
}

/// What to do with a line after history expansion.
pub enum Expanded {
    /// Go on with the line: as it was read, or with its history references
    /// replaced (and then echoed).
    Line(Vec<u8>),
    /// Read the line again, starting from its expansion (`history.verify`).
    Again,
    /// Drop the command: expansion failed (and the error was reported), or
    /// `:p` printed the line and added it to the history.
    Drop,
}

/// History expansion (`history.expand`, see `bang.rs`) of a line read by
/// the editor, after `pending`, the lines read before it of the command.
pub fn expand_history(sh: &Shell, pending: &[u8], line: Vec<u8>) -> Expanded {
    let r = with_history(|h| BANG.with_borrow_mut(|mem| bang::expand(h, mem, pending, &line)));
    match r {
        None | Some(Ok(None)) => Expanded::Line(line),
        Some(Err(msg)) => {
            sh.error(msg);
            Expanded::Drop
        }
        Some(Ok(Some(x))) if x.print => {
            sys::write_all(2, &x.text);
            add_history(sh, &[pending, &x.text].concat());
            Expanded::Drop
        }
        Some(Ok(Some(x))) if sh.opt(Opt::HistVerify) && EDITS.get() => {
            let text = x.text.strip_suffix(b"\n").unwrap_or(&x.text);
            REFILL.set(Some(String::from_utf8_lossy(text).into_owned()));
            Expanded::Again
        }
        Some(Ok(Some(x))) => {
            sys::write_all(2, &x.text);
            Expanded::Line(x.text)
        }
    }
}

/// Before each prompt: applies `HISTSIZE`, and with `history.share` reads
/// what other shells have added to the history file.
fn update_history(sh: &Shell) {
    let size = history_size(sh);
    let file = history_file(sh).filter(|_| sh.opt(Opt::ShareHistory));
    with_history(|h| {
        let _ = h.set_max_len(size);
        if let Some(f) = file {
            h.sync(&f);
        }
    });
}

/// Runs `f` on the history. Returns None if there is no line editor (and
/// so no history), or if it is reading a line (and a completer runs `fc`).
/// `f` must not run commands, which may use the history.
pub fn with_history<R>(f: impl FnOnce(&mut ShellHistory) -> R) -> Option<R> {
    EDITOR.with(|e| e.try_borrow_mut().ok()?.as_mut().map(|ed| f(ed.history_mut())))
}

/// `$HISTCMD`: 0 without a history, as in zsh.
pub fn histcmd() -> usize {
    with_history(|h| h.current_event()).unwrap_or(0)
}

/// The prompt: `PS2` for a continuation line, otherwise `PS1`, built with
/// the extensions' prompt hooks and the plugins' files if there are any.
pub fn prompt(sh: &mut Shell, continuation: bool) -> crate::prompt::Prompt {
    prompts(sh, continuation, false).0
}

/// The prompt, and with `right` the right prompt that goes with it, if one
/// is set: expanded with the same prompt variables of the plugins.
fn prompts(sh: &mut Shell, continuation: bool, right: bool) -> (crate::prompt::Prompt, Option<crate::prompt::Prompt>) {
    if continuation {
        return (sh.prompt(b"PS2"), right.then(|| sh.right_prompt(true)).flatten());
    }
    match crate::plugins::prompt(sh, right) {
        Ok(Some(prompts)) => prompts,
        Err(crate::shell::Flow::Exit(n)) => sh.exit(n),
        _ => (sh.prompt(b"PS1"), right.then(|| sh.right_prompt(false)).flatten()),
    }
}

/// The names the completer needs, taken from the shell before each prompt.
fn names(sh: &Shell) -> Names {
    Names {
        functions: sh.functions.keys().cloned().collect(),
        aliases: sh.aliases.clone(),
        vars: (sh.vars.names().cloned())
            .chain(sh.vars.special_names().map(<[u8]>::to_vec))
            .collect(),
        path: sh.get_var(b"PATH").unwrap_or_default(),
        home: sh.get_var(b"HOME"),
        completers: crate::plugins::completer_names(sh),
        builtins: crate::plugins::builtin_names(sh),
        jobs: (sh.jobs.order().iter())
            .map(|&i| (JobTable::number(i), sh.jobs.get(i).text().into_bytes()))
            .collect(),
        plugins: crate::plugins::loaded_names(sh),
        plugin_dir: crate::plugins::plugin_dir(sh),
        cdpath: sh.get_var(b"CDPATH").unwrap_or_default(),
        autocd: sh.opt(crate::options::Opt::Autocd),
        history_expand: sh.opt(Opt::HistExpand),
        options: crate::options::Options::all_names()
            .filter(|o| !matches!(o.0, crate::options::Opt::Interactive | crate::options::Opt::Stdin))
            .map(|(o, name)| (name, sh.opt(o)))
            .collect(),
    }
}

/// Runs an extension's completer for the editor. The terminal is in raw mode
/// and belongs to the editor, so the commands the completer runs are not
/// jobs: like those of `$(...)`, they don't save or restore its modes.
fn ask(words: &[Vec<u8>], index: usize) -> Completion {
    let p = SHELL.get();
    if p.is_null() || EXIT.get().is_some() {
        return Completion::Default;
    }
    // SAFETY: `read_line` set the pointer from its `&mut Shell`, which it
    // doesn't use while the editor runs, and resets it after.
    let sh = unsafe { &mut *p };
    let jobctl = sh.jobctl.take();
    let r = crate::plugins::complete(sh, words, index);
    sh.jobctl = jobctl;
    match r {
        Ok(c) => c,
        Err(Flow::Exit(n)) => {
            EXIT.set(Some(n));
            Completion::Failed
        }
        Err(_) => Completion::Failed,
    }
}

/// Expands a word of the line for the editor (`expand-or-complete`), as an
/// argument of a command. Errors are reported, and give None.
fn expand(word: &[u8]) -> Option<Vec<Vec<u8>>> {
    let p = SHELL.get();
    if p.is_null() || EXIT.get().is_some() {
        return None;
    }
    // A command name, so that the word isn't a reserved word, an alias
    // or an assignment.
    let mut text = b": ".to_vec();
    text.extend_from_slice(word);
    let mut parser = crate::lexer::Parser::new(text, 1, true);
    // SAFETY: as in `ask`.
    let sh = unsafe { &mut *p };
    parser.bareglobqual = sh.opt(Opt::Bareglobqual);
    let list = parser.parse_all().ok()?;
    let [cmd] = &list[..] else { return None };
    let ([pipeline], true) = (&cmd.list.first.cmds[..], cmd.list.rest.is_empty() && !cmd.async_) else {
        return None;
    };
    let crate::ast::Command::Simple(simple) = pipeline else {
        return None;
    };
    let [_, w] = &simple.words[..] else { return None };
    if !simple.assigns.is_empty() || !simple.redirs.is_empty() {
        return None;
    }
    let jobctl = sh.jobctl.take();
    let r = sh.expand_words(std::slice::from_ref(w));
    sh.jobctl = jobctl;
    match r {
        Ok(fields) => Some(fields),
        Err(Flow::Exit(n)) => {
            EXIT.set(Some(n));
            None
        }
        Err(_) => None,
    }
}

/// The subscripts of the array `name` and its elements, for completing
/// `${name[`: an associative array's keys, or else indices (a string is
/// one element).
fn subscripts(name: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let p = SHELL.get();
    if p.is_null() {
        return None;
    }
    // SAFETY: as in `ask`.
    let sh = unsafe { &*p };
    let elements = match sh.vars.get_value(name) {
        Some(Value::Assoc(h)) => return Some(h.keys().iter().cloned().zip(h.values().iter().cloned()).collect()),
        Some(v) => v.elements().to_vec(),
        None => sh.special_elements(name)?,
    };
    let indices = (0..elements.len()).map(|i| i.to_string().into_bytes());
    Some(indices.zip(elements).collect())
}

/// A match for the word under the cursor: the text that replaces the word
/// in the line, and its description.
pub type Match = (String, Option<String>);

/// The matches Tab offers for the word at the end of `line`, as the line
/// editor would complete it (for `__luish_internal complete`). None if a
/// completer failed (it printed why).
pub fn completions(sh: &mut Shell, line: &[u8]) -> Result<Option<Vec<Match>>, Flow> {
    let mut helper = ShellHelper::default();
    helper.names = names(sh);
    helper.ask = Some(ask);
    helper.expand = Some(expand);
    helper.subscripts = Some(subscripts);
    let outer = SHELL.replace(sh as *mut Shell);
    let r = helper.completions(line);
    SHELL.set(outer);
    if let Some(n) = EXIT.take() {
        return Err(Flow::Exit(n));
    }
    Ok(r.map(|items| items.into_iter().map(|i| (i.replacement, i.desc)).collect()))
}

/// The highlighting colours, or None if highlighting is off.
fn colors(sh: &Shell) -> Option<highlight::Colors> {
    if sh.get_var(b"NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return None;
    }
    highlight::Colors::parse(&sh.get_var(b"LUISH_HIGHLIGHT").unwrap_or_default())
}

/// Reads a line with the editor. `pending` is the text read so far of an
/// incomplete command, which the highlighter continues from.
pub fn read_line(sh: &mut Shell, continuation: bool, pending: &[u8]) -> Line {
    if !continuation {
        update_history(sh);
    }
    let (p, right) = prompts(sh, continuation, true);
    let text = String::from_utf8_lossy(&p.text).into_owned();
    // The line editor measures the prompt without its escape sequences.
    let plain = p.plain.map(|s| String::from_utf8_lossy(&s).into_owned());
    let vi = sh.opt(Opt::Vi);
    // Not for the continuation lines of a command.
    let suggest = sh.opt(Opt::Autosuggest) && !continuation;
    let names = names(sh);
    let colors = colors(sh);
    let wordchars = sh
        .get_var(b"WORDCHARS")
        .map(|w| String::from_utf8_lossy(&w).into_owned());
    let keymap = sh.keymap.clone();
    // zsh's default leaves the last column free.
    let indent = right.as_ref().map_or(1, |_| {
        let v = sh.get_var(b"ZLE_RPROMPT_INDENT").unwrap_or_default();
        std::str::from_utf8(&v).ok().and_then(|v| v.parse().ok()).unwrap_or(1)
    });
    let transient = sh.opt(Opt::TransientRprompt);
    SHELL.set(sh as *mut Shell);
    let line = EDITOR.with(|e| {
        let mut e = e.borrow_mut();
        let Some(ed) = e.as_mut() else {
            return Line::Eof;
        };
        let Some((menu, keys)) = ed.helper().map(|h| (h.menu.clone(), h.keys.clone())) else {
            return Line::Eof;
        };
        keys::update(ed, &menu, &keys, &keymap, wordchars);
        let refill = REFILL
            .take()
            .or_else(|| (!continuation).then(|| PUSHED.with_borrow_mut(Vec::pop)).flatten());
        // The history entry to start with, after `accept-line-and-down-history`.
        let initial = (!continuation && refill.is_none())
            .then(|| NEXT.take())
            .flatten()
            .and_then(|n| Some((ed.history().index_of(n)?, ed.history().event(n)?.to_owned())));
        if let (Some((i, _)), Ok(mut k)) = (&initial, keys.lock()) {
            k.prefilled = Some(*i);
        }
        if let Some(h) = ed.helper_mut() {
            h.names = names;
            h.prompt.clone_from(plain.as_ref().unwrap_or(&text));
            if let Ok(mut m) = h.menu.lock() {
                m.close();
            }
            h.highlight.colors = colors;
            h.suggest = suggest;
            h.right.set(right.as_ref(), indent, transient);
            h.highlight.context.clear();
            h.highlight.context.extend_from_slice(pending);
            h.highlight.known.get_mut().clear();
        }
        ed.set_edit_mode(if vi {
            rustyline::EditMode::Vi
        } else {
            rustyline::EditMode::Emacs
        });
        // How long after Esc another key makes it a Meta key, as zsh's
        // `KEYTIMEOUT` (without one, rustyline waits for the next key), so
        // that Esc alone takes effect, and closes the completion menu. In vi
        // insert mode Esc then a key is the same as Meta and the key, so
        // the wait can be shorter.
        ed.set_keyseq_timeout(Some(if vi { 100 } else { 400 }));
        let start = refill
            .as_deref()
            .or(initial.as_ref().map(|(_, t)| t.as_str()))
            .unwrap_or("");
        let r = match &plain {
            Some(plain) => ed.readline_with_initial(&(plain, &text), (start, "")),
            None => ed.readline_with_initial(&text, (start, "")),
        };
        let down = keys.lock().ok().and_then(|mut k| k.down.take());
        if let (Ok(_), Some(i)) = (&r, down)
            && i + 1 < ed.history().len()
        {
            NEXT.set(Some(ed.history().event_at(i + 1)));
        }
        match r {
            Ok(mut l) => {
                l.push('\n');
                Line::Text(l.into_bytes())
            }
            Err(ReadlineError::Interrupted) => Line::Interrupted,
            Err(_) => Line::Eof,
        }
    });
    SHELL.set(std::ptr::null_mut());
    if let Some(n) = EXIT.take() {
        sh.exit(n);
    }
    line
}

/// Reads a line from standard input, a byte at a time (so that nothing
/// past it is read), for a question asked on stderr. `None` at end of file
/// or on Ctrl-C, after starting a new line on stderr.
pub fn read_answer() -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut buf = [0u8; 1];
    loop {
        match sys::read(0, &mut buf, true) {
            Ok(1) if buf[0] == b'\n' => return Some(line),
            Ok(1) => line.push(buf[0]),
            Err(libc::EINTR) if !crate::signals::is_pending(libc::SIGINT) => {}
            _ => {
                sys::write_all(2, b"\n");
                return None;
            }
        }
    }
}

/// Sources a file in the current shell if it exists.
pub fn source_file(sh: &mut Shell, path: &[u8]) {
    if let Ok(text) = std::fs::read(to_path(path)) {
        run_file(sh, path, &text);
    }
}

/// Runs the text of a startup file in the current shell.
pub fn run_file(sh: &mut Shell, path: &[u8], text: &[u8]) {
    use crate::frames::{Frame, FrameKind, SourceFile};
    let file = SourceFile::new(path, sh.curdir.as_deref());
    let in_string = sh.push_frame(Frame::file(FrameKind::Source, file, 0));
    run_text(sh, text, true);
    sh.pop_frame(in_string);
}

/// [`run_file`], without expanding aliases if not `aliases`.
pub fn run_text(sh: &mut Shell, text: &[u8], aliases: bool) {
    let saved = sh.lineno;
    sh.lineno = 1;
    let r = sh.run_text(text, aliases);
    sh.lineno = saved;
    if let Err(crate::shell::Flow::Exit(n)) = r {
        sh.exit(n);
    }
}

/// Runs the startup files of a login shell (interactive or not, as in
/// dash; after `rc.d`): the cached files of `luish/login.d` if it exists in the
/// configuration directory (see `startcache.rs`), otherwise `/etc/profile`
/// and `~/.profile`.
pub fn login_profiles(sh: &mut Shell) {
    if let Some(dir) = crate::startcache::config_dir(sh, b"login.d") {
        crate::startcache::run(sh, &dir, b"login", None);
        return;
    }
    source_file(sh, b"/etc/profile");
    if let Some(mut home) = sh.get_var(b"HOME") {
        home.extend_from_slice(b"/.profile");
        source_file(sh, &home);
    }
}

/// Runs the cached files of `luish/rc.d` and `config.toml`, with the
/// plugins it enables, for an interactive shell, before the login files
/// (see `startcache.rs`). Without `rc.d`, `config.toml` is cached alone.
pub fn rc_d(sh: &mut Shell) {
    let Some(config) = crate::config::path(sh) else {
        return;
    };
    let dir = crate::startcache::config_dir(sh, b"rc.d").or_else(|| {
        sys::stat(&config)
            .is_some()
            .then(|| [parent_dir(&config), b"/rc.d"].concat())
    });
    if let Some(dir) = dir {
        crate::startcache::run(sh, &dir, b"rc", Some(&config));
    }
}

fn parent_dir(path: &[u8]) -> &[u8] {
    &path[..path.iter().rposition(|&c| c == b'/').unwrap_or(0)]
}

/// Reads the startup files of an interactive shell: `$ENV`, then luish's
/// own `luishrc`. Both always run, but their `__luish_cache` blocks, and
/// those of the plugins they load, are cached (`startcache::begin_startup`).
pub fn startup(sh: &mut Shell) {
    let mut run = crate::startcache::begin_startup(sh);
    if let Some(env) = sh.get_var(b"ENV")
        && let Ok(w) = crate::lexer::parse_string_word(&env)
        && let Ok(path) = sh.expand_word_str(&w)
    {
        crate::startcache::run_mixed(sh, &mut run, &path, None);
    }
    let config = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config");
    if let Some(mut c) = config {
        c.extend_from_slice(b"/luish/luishrc");
        if sys::stat(&c).is_some() {
            crate::startcache::run_mixed(sh, &mut run, &c, None);
        }
    }
    crate::startcache::finish_startup(sh, run);
}
