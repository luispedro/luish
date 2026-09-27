# luish

luish is a shell for Linux, written in Rust. It runs POSIX scripts as [dash](http://gondor.apana.org.au/~herbert/dash/)
does, at dash's speed, and adds what you expect from zsh when you type commands: line editing with zsh's keys,
syntax highlighting, autosuggestions, a shared history in zsh's format, Tab completion with a menu, zsh's prompt
sequences and glob qualifiers.

luish is usable as a daily shell. It implements the POSIX shell language and built-ins (GNU `configure` scripts give
the same results as under dash), job control, and the interactive features above. The few differences from dash, and
what is still missing, are listed in [](compatibility.md).

## Highlights

**A modern plugin architecture.** Plugins are directories of shell files, as zsh users know them, plus optional code
in [Rhai](https://rhai.rs), a small embedded language, for hooks, prompt variables and Tab completion. `config.toml`
lists them, `plugin sync` fetches them from git, and `plugins.lock` pins their commits, so every machine gets the same
setup. A shell that loads no plugins pays nothing for them. See [](plugins.md).

**As fast as dash, with zsh's features.** Scripts run as fast as under dash, and faster when they do arithmetic or
call many functions; bash and zsh take up to five times as long. The interactive features are opt-in and cost
nothing in scripts. See [](performance.md).

**Instant startup, however much your startup files do.** luish can cache the *effect* of your startup files: the
variables, functions, aliases and options that they set. A new shell restores that state in a few milliseconds instead
of running `conda`'s, `nvm`'s and other initialization scripts again, and reruns them only when one of the files
changes. See [Cached startup files](usage.md#cached-startup-files).

**Modern configuration.** Settings go in a [TOML](https://toml.io) file, `~/.config/luish/config.toml`, and luish's own
options are named in groups (`history.share`, `glob.star`, `cd.auto`), which are tables in the file:

```toml
[options.history]
file = "~/.histfile"
share = true

[alias]
ll = "ls -l"

[plugins.enabled]
std.completion = "*"         # completion for common commands, and git
```

Settings come in layers: `config.toml` first, then the plugins it enables (which can bundle a set of options), then
your own startup files, each able to override the one before. See [Settings in
config.toml](usage.md#settings-in-configtoml).

[](improvements.md) has more on these, and on the smaller annoyances of other shells that luish fixes.

## Getting started

Install luish (see [](installation.md)), and start it:

```sh
luish                             # an interactive shell
luish script.sh                   # run a script, as sh script.sh does
```

Then read [](usage.md) for the command line, the prompt, line editing, history, completion and startup files, and
[](plugins.md) to extend it. In the shell, `help` lists the built-in commands and `help NAME` explains one; the same
text is in [](builtins.md).

```{toctree}
:hidden:
:maxdepth: 2

installation
usage
globbing
builtins
plugins
improvements
performance
compatibility
whatsnew
```
