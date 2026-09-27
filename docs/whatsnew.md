# What's new

## Version 0.1.0 (27 September 2026)

The first release.

**Scripts.** The POSIX shell language and its built-ins, plus `local`, behave
as in dash. GNU `configure` scripts give the same results as under dash.

Scripts run as fast as under dash (see [](performance.md)).

**Startups are cached**: a new terminal is started in &lt; 20 ms even when
using frameworks like `conda` and `nvm` that set many options.

**Interactive use.**

- Line editing with zsh's emacs keys (or vi keys), syntax highlighting, and
  autosuggestions from the history (`editor.autosuggest`).
- History in zsh's file format, which luish and zsh can share, optionally
  between running shells (`history.share`), with `fc` and search as you type.
- Tab completion with an interactive menu and a standard library of completions
  for &gt; 70 common commands and support for falling back on bash-completion
  scripts.
- zsh's prompt sequences (`%~`, `%?`, `%F{...}`, conditions, ...), improved
  with better variable names.
- Job control (`jobs`, `fg`, `bg`, Ctrl-Z), and zsh's directory stack (`pushd`, `popd`, `dirs`).
- zsh's recursive globbing (`**/`) and glob qualifiers (`*(.om[0])`).
- `help` for every [builtin](builtins.md)

**Configuration.** Settings go in `~/.config/luish/config.toml` and
`~/.config/luish/rc.d`.

**Plugins.** Plugins are directories of shell files, with optional code in
[Rhai](https://rhai.rs) for hooks, prompt variables and Tab completion. The can
be automatically installed from GitHub and can be pinned to specific commits.
This is built-in to luish rather than built with a separate plugin manager.


