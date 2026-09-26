//! Interactive mode: prompts, the line editor, history, and startup files.

mod complete;
mod highlight;
pub mod history;

use std::cell::RefCell;

use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};

use complete::{Names, ShellHelper};
use history::ShellHistory;

use crate::input::Line;
use crate::options::Opt;
use crate::shell::Shell;
use crate::sys;

thread_local! {
    static EDITOR: RefCell<Option<Editor<ShellHelper, ShellHistory>>> = const { RefCell::new(None) };
}

fn history_file(sh: &Shell) -> Option<Vec<u8>> {
    sh.get_var(b"HISTFILE").filter(|h| !h.is_empty())
}

pub fn to_path(b: &[u8]) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b).into()
}

/// Sets up the line editor. Returns false if it can't be used.
pub fn init_editor(sh: &Shell) -> bool {
    let Ok(mut ed) = Editor::with_history(Config::default(), ShellHistory::default()) else {
        return false;
    };
    ed.set_helper(Some(ShellHelper::default()));
    ed.set_completion_type(CompletionType::List);
    let size = sh
        .get_var(b"HISTSIZE")
        .and_then(|s| String::from_utf8(s).ok()?.parse().ok())
        .unwrap_or(1000);
    let _ = ed.set_max_history_size(size);
    if let Some(h) = history_file(sh) {
        let _ = ed.load_history(&to_path(&h));
    }
    EDITOR.with(|e| *e.borrow_mut() = Some(ed));
    true
}

pub fn save_history(sh: &Shell) {
    let Some(h) = history_file(sh) else { return };
    EDITOR.with(|e| {
        if let Some(ed) = e.borrow_mut().as_mut() {
            let _ = ed.save_history(&to_path(&h));
        }
    });
}

/// Adds the text of a command about to be run to the history.
pub fn add_history(text: &[u8]) {
    let text = String::from_utf8_lossy(text);
    let text = text.trim_end_matches('\n');
    if text.trim().is_empty() {
        return;
    }
    with_history(|h| h.add_current(text));
}

/// Runs `f` on the history. Returns None if there is no line editor (and
/// so no history). `f` must not run commands, which may use the history.
pub fn with_history<R>(f: impl FnOnce(&mut ShellHistory) -> R) -> Option<R> {
    EDITOR.with(|e| e.borrow_mut().as_mut().map(|ed| f(ed.history_mut())))
}

/// The prompt: `PS2` for a continuation line, otherwise the one a plugin's
/// `prompt` hook gives (which isn't parameter-expanded), or else `PS1`.
pub fn prompt(sh: &mut Shell, continuation: bool) -> crate::prompt::Prompt {
    if continuation {
        return sh.prompt(b"PS2");
    }
    match crate::plugins::prompt(sh) {
        Ok(Some(text)) => sh.percent_expand_prompt(text),
        Err(crate::shell::Flow::Exit(n)) => sh.exit(n),
        _ => sh.prompt(b"PS1"),
    }
}

/// The names the completer needs, taken from the shell before each prompt.
fn names(sh: &Shell) -> Names {
    Names {
        commands: sh.functions.keys().chain(sh.aliases.keys()).cloned().collect(),
        vars: sh.vars.names().cloned().collect(),
        path: sh.get_var(b"PATH").unwrap_or_default(),
        home: sh.get_var(b"HOME"),
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
    let p = prompt(sh, continuation);
    let text = String::from_utf8_lossy(&p.text).into_owned();
    // The line editor measures the prompt without its escape sequences.
    let plain = p.plain.map(|s| String::from_utf8_lossy(&s).into_owned());
    let vi = sh.opt(Opt::Vi);
    let names = names(sh);
    let colors = colors(sh);
    EDITOR.with(|e| {
        let mut e = e.borrow_mut();
        let Some(ed) = e.as_mut() else {
            return Line::Eof;
        };
        if let Some(h) = ed.helper_mut() {
            h.names = names;
            h.highlight.colors = colors;
            h.highlight.context.clear();
            h.highlight.context.extend_from_slice(pending);
            h.highlight.known.get_mut().clear();
        }
        ed.set_edit_mode(if vi {
            rustyline::EditMode::Vi
        } else {
            rustyline::EditMode::Emacs
        });
        let r = match &plain {
            Some(plain) => ed.readline(&(plain, &text)),
            None => ed.readline(&text),
        };
        match r {
            Ok(mut l) => {
                l.push('\n');
                Line::Text(l.into_bytes())
            }
            Err(ReadlineError::Interrupted) => Line::Interrupted,
            Err(_) => Line::Eof,
        }
    })
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
        crate::startcache::run(sh, &dir, b"login");
        return;
    }
    source_file(sh, b"/etc/profile");
    if let Some(mut home) = sh.get_var(b"HOME") {
        home.extend_from_slice(b"/.profile");
        source_file(sh, &home);
    }
}

/// Runs the cached files of `luish/rc.d`, if it exists, for an interactive
/// shell, before the login files (see `startcache.rs`).
pub fn rc_d(sh: &mut Shell) {
    if let Some(dir) = crate::startcache::config_dir(sh, b"rc.d") {
        crate::startcache::run(sh, &dir, b"rc");
    }
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
