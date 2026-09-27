# What's new

## Version 0.1.0 (27 September 2026)

The first release. luish is a POSIX shell for Linux, as fast as dash, with the interactive features of a zsh setup.
Binaries are available for x86_64 and aarch64 Linux, in a glibc and a static musl build (see [](installation.md)).

**Scripts.** The POSIX shell language and its built-ins, plus `local`, behave as in dash, which is the reference where
POSIX leaves a choice: GNU `configure` scripts give the same results as under dash. Scripts run as fast as under dash,
and faster when they do arithmetic or call many functions (see [](performance.md)). The few deliberate differences
from dash, mostly following zsh, are listed in [](compatibility.md).

**Interactive use.**

- Line editing with zsh's emacs keys (or vi keys), which `bindkey` can change, syntax highlighting, and
  autosuggestions from the history (`editor.autosuggest`).
- History in zsh's file format, which luish and zsh can share, optionally between running shells (`history.share`),
  with `fc` and search as you type.
- Tab completion with a menu, of commands, files, variables and more; commands newly installed in `PATH` are found
  without `hash -r`.
- zsh's prompt sequences (`%~`, `%?`, `%F{...}`, conditions, ...).
- Job control (`jobs`, `fg`, `bg`, Ctrl-Z), and zsh's directory stack (`pushd`, `popd`, `dirs`).
- zsh's recursive globbing (`**/`) and glob qualifiers (`*(.om[0])`), as options (see [](globbing.md)).
- `help` for every built-in (see [](builtins.md)).

**Configuration.** Settings go in `~/.config/luish/config.toml`, and luish's own options are named in groups
(`history.share`, `glob.star`), with zsh's names as aliases. Startup files can be cached: a new shell restores the
variables, functions, aliases and options they set in a few milliseconds, and runs them again only when one of them
changes (see [Cached startup files](usage.md#cached-startup-files)).

**Plugins.** Plugins are directories of shell files, with optional code in [Rhai](https://rhai.rs) for hooks, prompt
variables and Tab completion. `config.toml` lists them, `plugin sync` fetches them from git and `plugins.lock` pins
their versions. The standard collection, `luish-std-plugins`, completes the options and arguments of about 70 common
commands and of git, and can use bash-completion for about a thousand more (see [](plugins.md)).

What is still missing is listed in [](compatibility.md#known-limitations).
