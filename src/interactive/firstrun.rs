//! The first run (DEVELOPING.md): an interactive shell on a terminal whose
//! configuration directory (`$XDG_CONFIG_HOME/luish`) is missing or empty
//! offers to write `config.toml` there, before reading it: the recommended
//! settings (the standard plugins, autosuggestions, `prompt.percent`,
//! `history.expand` and `history.file`, commented out, at its default), the same file with
//! everything commented out (so the question isn't asked again), a
//! minimal file with only a personal plugin (as `plugin add` takes it), or
//! nothing (so the question is asked again), in a menu.

use super::tty::{Raw, read_byte, readable};
use crate::shell::Shell;
use crate::sys;

/// Where bridge.bash, in std.bash-completion, looks for bash-completion.
const BASH_COMPLETION: &[&str] = &[
    "/usr/share/bash-completion/bash_completion",
    "/usr/local/share/bash-completion/bash_completion",
    "/opt/homebrew/share/bash-completion/bash_completion",
];

/// Whether bash-completion is installed where std.bash-completion finds it.
fn has_bash_completion(sh: &Shell) -> bool {
    let readable = |p: &[u8]| !p.is_empty() && sys::access(p, libc::R_OK);
    sh.get_var(b"BASH_COMPLETION_SCRIPT").is_some_and(|p| readable(&p))
        || BASH_COMPLETION.iter().any(|p| readable(p.as_bytes()))
        || sh
            .get_var(b"HOME")
            .is_some_and(|h| readable(&[&h[..], b"/.nix-profile/share/bash-completion/bash_completion"].concat()))
}

/// `path` with `~` for the home directory, for messages and for
/// `config.toml` (whose strings expand a leading `~`).
fn with_tilde(sh: &Shell, path: &[u8]) -> String {
    let home = sh.get_var(b"HOME").filter(|h| h.len() > 1);
    match home.as_deref().and_then(|h| path.strip_prefix(h)) {
        Some(rest) if rest.is_empty() || rest[0] == b'/' => format!("~{}", String::from_utf8_lossy(rest)),
        _ => String::from_utf8_lossy(path).into_owned(),
    }
}

/// The text of `config.toml`: the recommended settings, commented out
/// unless `on`. `history` is the default history file, shown commented
/// out, and `bash_completion`
/// whether to enable std.bash-completion.
fn config_text(history: &str, bash_completion: bool, on: bool) -> String {
    let history = crate::config::toml_str(history);
    let mut lines = vec![
        "# luish's configuration: see https://luish.readthedocs.io/en/latest/getting-started.html".to_string(),
        "# (written by luish when it first ran).".into(),
    ];
    if !on {
        lines.push("# Remove the # at the start of a line to use it.".into());
    }
    // A setting starts with `=`, which stands for `# ` when it is off.
    let settings = [
        "",
        "# Suggest the rest of the line from the history, in grey; Right or End accepts it.",
        "=[options.editor]",
        "=autosuggest = true",
        "",
        "# zsh's % sequences in prompts: PS1='%~ %# ' shows the directory, then % (# for root).",
        "=[options.prompt]",
        "=percent = true",
        "",
        "# !! for the previous command, !$ for its last word, ^old^new to run it with old replaced by new.",
        "=[options.history]",
        "=expand = true",
        &format!("# file = {history}   # the default"),
    ];
    let plugins = [
        "",
        "# The standard plugins: plugin sync fetches them, and plugin update updates them.",
        "=[plugins.enabled]",
        "=std.completion = \"*\"         # Tab completion for about 230 commands, and git",
        match bash_completion {
            true => "=std.bash-completion = \"*\"    # completion from bash-completion for the others",
            false => "# std.bash-completion = \"*\"  # completion from bash-completion (which isn't installed)",
        },
    ];
    let plugins: &[&str] = if cfg!(feature = "plugins") { &plugins } else { &[] };
    for line in settings.iter().chain(plugins) {
        lines.push(match line.strip_prefix('=') {
            Some(l) if on => l.to_string(),
            Some(l) => format!("# {l}"),
            None => line.to_string(),
        });
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Asks `question` on stderr. `None` for Ctrl-C or Ctrl-D.
fn ask(question: &str) -> Option<String> {
    sys::write_all(2, question.as_bytes());
    let answer = super::read_answer();
    // Ctrl-C only stops the question.
    crate::signals::clear_pending(libc::SIGINT as usize);
    answer.map(|a| String::from_utf8_lossy(a.trim_ascii()).into_owned())
}

/// Whether the configuration directory `dir` is missing or empty.
fn is_empty(dir: &[u8]) -> bool {
    match sys::read_dir(dir) {
        Some(names) => names.iter().all(|n| n == b"." || n == b".."),
        None => sys::stat(dir).is_none(),
    }
}

/// A key pressed in the menu.
#[derive(Debug, PartialEq)]
enum Key {
    Up,
    Down,
    Enter,
    /// `1` to `9`.
    Digit(usize),
    /// Ctrl-C, Ctrl-D, Esc, `q`, or the end of the input.
    Cancel,
    Other,
}

/// Reads a key from the terminal, in raw mode. An escape sequence is read
/// whole; Esc alone is one with nothing after it for 50 ms.
fn read_key() -> Key {
    match read_byte() {
        None | Some(3 | 4 | b'q') => Key::Cancel,
        Some(b'\r' | b'\n') => Key::Enter,
        Some(b'k' | 0x10) => Key::Up,
        Some(b'j' | 0x0e | b'\t') => Key::Down,
        Some(c @ b'1'..=b'9') => Key::Digit((c - b'0') as usize),
        Some(0x1b) if !readable(50) => Key::Cancel,
        Some(0x1b) => match read_byte() {
            Some(b'[' | b'O') => {
                // The parameters, then the final byte (`A` in `ESC [ A`).
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
        Some(_) => Key::Other,
    }
}

/// The lines of the menu, with the item `sel` highlighted.
fn menu_lines(items: &[String], sel: usize) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        out.push_str("\r\x1b[K");
        match i == sel {
            true => out.push_str(&format!("\x1b[7m> {}. {item}\x1b[m\n", i + 1)),
            false => out.push_str(&format!("  {}. {item}\n", i + 1)),
        }
    }
    out
}

/// Lets the user choose one of `items` (numbered from 0) on stderr, with
/// Up and Down and Enter, or its number. `None` for Ctrl-C, Ctrl-D or Esc.
/// On a terminal that can't move the cursor (`TERM` unset or `dumb`) the
/// number is typed instead, and Enter alone chooses the first.
fn choose(sh: &Shell, items: &[String]) -> Option<usize> {
    let dumb = sh.get_var(b"TERM").is_none_or(|t| t.is_empty() || t == b"dumb");
    let raw = if dumb { None } else { Raw::new() };
    if raw.is_none() {
        let list: String = (items.iter().enumerate())
            .map(|(i, item)| format!("  {}. {item}\n", i + 1))
            .collect();
        sys::write_all(2, list.as_bytes());
        loop {
            let a = ask(&format!("Choose 1-{} [1]: ", items.len()))?;
            match a.parse::<usize>() {
                _ if a.is_empty() => return Some(0),
                Ok(n) if (1..=items.len()).contains(&n) => return Some(n - 1),
                _ => {}
            }
        }
    }
    sys::write_all(2, b"(Up and Down to choose, Enter to confirm)\n");
    let mut sel = 0;
    sys::write_all(2, menu_lines(items, sel).as_bytes());
    let chosen = loop {
        match read_key() {
            Key::Up => sel = sel.checked_sub(1).unwrap_or(items.len() - 1),
            Key::Down => sel = (sel + 1) % items.len(),
            Key::Enter => break Some(sel),
            Key::Digit(n) if n <= items.len() => {
                sel = n - 1;
                break Some(sel);
            }
            Key::Cancel => break None,
            _ => continue,
        }
        let up = format!("\x1b[{}A", items.len());
        sys::write_all(2, [up, menu_lines(items, sel)].concat().as_bytes());
    };
    if chosen.is_some() {
        let up = format!("\x1b[{}A", items.len());
        sys::write_all(2, [up, menu_lines(items, sel)].concat().as_bytes());
    }
    chosen
}

/// Offers to write `config.toml` if the configuration directory is
/// missing or empty.
pub fn run(sh: &mut Shell) {
    let Some(file) = crate::config::path(sh) else {
        return;
    };
    let dir = super::parent_dir(&file);
    if !is_empty(dir) {
        return;
    }
    let state = crate::startcache::xdg_dir(sh, b"XDG_STATE_HOME", b"/.local/state").unwrap_or_default();
    let history = with_tilde(sh, &[&state[..], b"/luish/history"].concat());
    let bash_completion = has_bash_completion(sh);
    let mut intro = format!(
        "Welcome to luish!\n\n\
         The configuration directory, {}, is empty.\n\n\
         I can immediately add the default configuration, which includes:\n",
        with_tilde(sh, dir)
    );
    if cfg!(feature = "plugins") {
        intro.push_str("  - Builtin Tab completion for about 230 commands\n");
        if bash_completion {
            intro.push_str("  - Bash-based completion for hundreds more commands\n");
        }
    }
    intro.push_str(
        "  - suggestions from the history as you type (Right accepts them)\n\
         \x20 - zsh's % sequences in prompts\n\n",
    );
    sys::write_all(2, intro.as_bytes());
    let mut items = vec![
        "Write the recommended configuration",
        "Write an empty configuration (so this question will not be asked again)",
    ];
    if cfg!(feature = "plugins") {
        items.push("Add a personal plugin (for more advanced users)");
    }
    items.push("Just start for now and ask again next time");
    let items: Vec<String> = items.into_iter().map(Into::into).collect();
    let skip = items.len() - 1;
    // The text to write, whether it enables anything, and for a
    // collection, what it holds.
    let (text, sync, note) = loop {
        match choose(sh, &items) {
            Some(0) => break (config_text(&history, bash_completion, true), true, None),
            Some(1) => break (config_text(&history, bash_completion, false), false, None),
            // A personal plugin, alone in a minimal configuration.
            Some(n) if n < skip => {
                let question = "Plugin (a GitHub repository, as OWNER/REPO or its URL, a git URL or a path): ";
                let Some(spec) = ask(question).filter(|s| !s.is_empty()) else {
                    continue;
                };
                let added = crate::plugins::prepare_addition(sh, &spec)
                    .and_then(|a| Ok((crate::plugins::add_to_config(sh, &file, "", &a)?, a.collection_note())));
                match added {
                    Ok((text, note)) => break (text, true, note),
                    Err(e) => sh.error(e),
                }
            }
            _ => return,
        }
    };
    let r = std::fs::create_dir_all(super::to_path(dir)).and_then(|_| std::fs::write(super::to_path(&file), &text));
    if let Err(e) = r {
        sh.error(format!("cannot write {}: {e}", String::from_utf8_lossy(&file)));
        return;
    }
    sys::write_all(2, format!("Wrote {}\n", with_tilde(sh, &file)).as_bytes());
    if sync && cfg!(feature = "plugins") {
        if crate::plugins::sync_config(sh) != 0 {
            sys::write_all(2, b"Run plugin sync once the plugins can be fetched\n");
        }
        sh.out(note.unwrap_or_default().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config() {
        let on = config_text("~/.local/state/luish/history", true, true);
        assert!(on.contains("\n[options.editor]\nautosuggest = true\n"), "{on}");
        assert!(
            on.contains("\n[options.history]\nexpand = true\n# file = \"~/.local/state/luish/history\""),
            "{on}"
        );
        #[cfg(feature = "plugins")]
        assert!(on.contains("\nstd.bash-completion = \"*\""), "{on}");
        let off = config_text("/data/luish/history", false, false);
        assert!(off.contains("\n# [options.editor]\n# autosuggest = true\n"), "{off}");
        assert!(off.lines().all(|l| l.is_empty() || l.starts_with('#')), "{off}");
        assert!(toml_span::parse(&on).is_ok(), "{on}");
    }
}
