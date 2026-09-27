//! Interactive mode: prompts, the line editor, history, and startup files.

mod complete;
mod highlight;
mod histfile;
pub mod history;
pub mod keys;
mod menu;

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
    ed.history_mut().search = helper.keys.lock().map(|k| k.search.clone()).unwrap_or_default();
    keys::bind(&mut ed, &helper.menu, &helper.keys);
    ed.set_helper(Some(helper));
    ed.set_completion_type(CompletionType::List);
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

/// The prompt: `PS2` for a continuation line, otherwise `PS1`, built with
/// the extensions' prompt hooks and the plugins' files if there are any.
pub fn prompt(sh: &mut Shell, continuation: bool) -> crate::prompt::Prompt {
    if continuation {
        return sh.prompt(b"PS2");
    }
    match crate::plugins::prompt(sh) {
        Ok(Some(prompt)) => prompt,
        Err(crate::shell::Flow::Exit(n)) => sh.exit(n),
        _ => sh.prompt(b"PS1"),
    }
}

/// The names the completer needs, taken from the shell before each prompt.
fn names(sh: &Shell) -> Names {
    Names {
        functions: sh.functions.keys().cloned().collect(),
        aliases: sh.aliases.clone(),
        vars: sh.vars.names().cloned().collect(),
        path: sh.get_var(b"PATH").unwrap_or_default(),
        home: sh.get_var(b"HOME"),
        completers: crate::plugins::completer_names(sh),
        jobs: (sh.jobs.order().iter())
            .map(|&i| (JobTable::number(i), sh.jobs.get(i).text().into_bytes()))
            .collect(),
        plugins: crate::plugins::loaded_names(sh),
        plugin_dir: crate::plugins::plugin_dir(sh),
        cdpath: sh.get_var(b"CDPATH").unwrap_or_default(),
        autocd: sh.opt(crate::options::Opt::Autocd),
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
    let p = prompt(sh, continuation);
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
        // The history entry to start with, after `accept-line-and-down-history`.
        let initial = (!continuation)
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
        let start = initial.as_ref().map_or("", |(_, t)| t.as_str());
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

/// Sources a file in the current shell if it exists.
pub fn source_file(sh: &mut Shell, path: &[u8]) {
    if let Ok(text) = std::fs::read(to_path(path)) {
        run_file(sh, &text);
    }
}

/// Runs the text of a startup file in the current shell.
pub fn run_file(sh: &mut Shell, text: &[u8]) {
    let saved = sh.lineno;
    sh.lineno = 1;
    let r = sh.run_string(text);
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
/// own `luishrc`.
pub fn startup(sh: &mut Shell) {
    if let Some(env) = sh.get_var(b"ENV")
        && let Ok(w) = crate::lexer::parse_string_word(&env)
        && let Ok(path) = sh.expand_word_str(&w)
    {
        source_file(sh, &path);
    }
    let config = crate::startcache::xdg_dir(sh, b"XDG_CONFIG_HOME", b"/.config");
    if let Some(mut c) = config {
        c.extend_from_slice(b"/luish/luishrc");
        if sys::stat(&c).is_some() {
            source_file(sh, &c);
        }
    }
}
