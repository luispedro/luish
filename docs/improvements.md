# Improvements over other shells

Luish improves on the traditional shells in ways small and large.

## A modern plugin architecture

In luish, plugins are part of the shell and managed in a modern way:
declaratively and reproducibly, with a lock file that records the exact commits
of each plugin.

- `plugin add` adds a plugin from GitHub, another git repository or a path to
  `config.toml` and fetches it.
- A plugin's `plugin.toml` lists its dependencies, which are loaded first, and
  can set options, aliases and key bindings.
- A plugin can have an extension, code written in [Rhai](https://rhai.rs) that
  runs inside the shell, avoiding the overhead of starting a separate process
  (see [](extensions.md)).
- Plugins are opt-in: a shell that loads none, as every script, pays nothing
  for them, and a plugin written only in shell doesn't start Rhai.


## As fast as dash, with zsh's features

bash and zsh are up to six times slower than dash on scripts that run mostly
inside the shell, which is why many systems use dash as `/bin/sh`. luish runs
scripts as fast as dash.

On the other hand, luish has many of zsh's functionality when running
interactively and can replace it for interactive use.

## bash's and zsh's scripting extensions, at dash's speed

POSIX `sh` has no arrays, `[[ ... ]]` or `${x/pattern/replacement}`, so scripts that need them are written for bash or
zsh, which are slow. luish runs them, with the same syntax and mostly the
same behaviour, up to six times faster than bash or zsh (see [](performance.md)). Indexed and associative arrays,
`typeset`, `[[ ... ]]`, zsh's parameter flags and bash's `${!name}` are always available, since they give a meaning to
what is a syntax error in dash, so POSIX scripts keep working. See [](usage.md#shell-language-extensions).

## Instant startup with cached startup files

Startup files tend to grow: `conda init`, `nvm`, `pyenv`, completion systems
and prompt frameworks each add something that runs whenever a shell starts.
This can add seconds to the startup time. Lazy-loading tricks can reduce the
time, but they add complexity and fricton. Luish instead caches the environment
so that new shells can start in a few milliseconds.

## Modern configuration

Instead of a script full of `setopt` lines with names such as
`hist_ignore_space`, luish's settings can go in a [TOML](https://toml.io) file,
`~/.config/luish/config.toml`, with its options, aliases, key bindings and
plugins:

```toml
[options]
autosuggest = true

[options.history]
file = "~/.histfile"
share = true
ignore_space = true

[alias]
ll = "ls -l"

[bindkey]
Up = "up-line-or-history"

[plugins.enabled]
std.completion = "*"         # completion for common commands, and git
```

luish's options are hierarchical: they are named in groups (`history.share`,
`glob.star`, `cd.auto`), which fits nicely with into TOML groups (zsh old names
are still supported as synonyms).


## Newly installed commands are found

Shells remember where they found each command in `PATH`, so they don't have to
search every directory each time you run it. The `hash` built-in shows that
list. In dash, bash and zsh, the list stays as it is until you change `PATH` or
run `hash -r` (`rehash` in zsh), even when the directories change on disk. Some
cases that go wrong:

- **A new command shadows an old one.** You ran `python` (found in `/usr/bin`),
  then installed another one in `~/.local/bin`, which comes earlier in your
  `PATH`. The shell keeps running `/usr/bin/python`.
- **A profile is switched.** Nix puts a directory in `PATH` that is really a
  symlink to the current version of your profile. Switching profiles can add a
  command that shadows a remembered one.

luish checks whether `PATH` directories have changed after each command line
you enter (not its contents, which can be slow, just mtimes) and automatically
runs `hash -r` if they have. You should never need to run `hash -r` manually.

The same check keeps Tab completion of command names up to date: new commands
in `PATH` are offered as soon as they are installed.
