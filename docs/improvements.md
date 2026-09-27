# Improvements over other shells

luish is a POSIX shell, and in scripts it behaves like dash. Beyond that, it improves on the traditional shells in a
few large ways, and fixes some of their long-standing annoyances. This page lists them.

## A modern plugin architecture

zsh's plugins are files to source, and plugin managers (oh-my-zsh, zinit, antidote and others) are themselves shell
code that clones repositories and sources their files, at every startup. In luish, plugins are part of the shell:

- A plugin can be plain shell files, as in zsh, and can add code in [Rhai](https://rhai.rs), a small language that
  runs inside the shell, for what is awkward or slow in shell: hooks (when the directory changes, after the startup
  files), variables for the prompt or a rewritten prompt, and Tab completion for a command's arguments. Rhai has
  modules for files and for git repositories (whose branch is read without starting `git`), and Ctrl-C stops it.
- `config.toml` lists the plugins to load, from GitHub, any git URL or a local directory. `plugin sync` fetches them
  and records their commits in `plugins.lock`, as a package manager does, so that another machine gets the same
  versions. Plugins are never fetched when a shell starts.
- A plugin's `plugin.toml` lists its dependencies, which are loaded first, and can set options, aliases and key
  bindings.
- Plugins are opt-in: a shell that loads none, as every script, pays nothing for them, and a plugin written only in
  shell doesn't start Rhai.

See [](plugins.md).

## As fast as dash, with zsh's features

bash and zsh are up to five times slower than dash on scripts that run mostly inside the shell, which is why many
systems use dash as `/bin/sh`. luish runs scripts as fast as dash, and faster when they do arithmetic or call many
functions, while its interactive side offers what zsh users expect: zsh's line-editing keys and `bindkey`, syntax
highlighting, autosuggestions, a history shared between terminals in zsh's file format, Tab completion with a menu,
zsh's prompt sequences, `**/` and glob qualifiers, global and suffix aliases, `setopt`, `pushd` and `autocd`. None of
this slows scripts down: it is either only in interactive shells or behind an option. The numbers are in
[](performance.md).

## Instant startup with cached startup files

Startup files tend to grow: `conda init`, `nvm`, `pyenv`, completion systems and prompt frameworks each add
something that runs whenever a shell starts, often by running a program, and a new terminal can take half a second
or more to show a prompt. zsh users reach for workarounds such as lazy-loading wrappers or instant-prompt tricks.

luish can instead cache what your startup files *do*. Put them in `~/.config/luish/rc.d/` (and `login.d/` for login
shells), and luish records the state they leave (variables, `PATH`, functions, aliases, options, traps) and restores
it in later shells, in a few milliseconds, whatever the files ran. The cache is rebuilt automatically when any of the
files (or the files they read with `.`, or `config.toml`, or luish itself) change. What must run every time, such as
starting `ssh-agent`, goes in `_uncached.lsh`. See [Cached startup files](usage.md#cached-startup-files).

## Modern configuration

Instead of a script full of `setopt` lines with names such as `hist_ignore_space`, luish's settings can go in a
[TOML](https://toml.io) file, `~/.config/luish/config.toml`, with its options, aliases, key bindings and plugins:

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

luish's own options are hierarchical: they are named in groups (`history.share`, `glob.star`, `cd.auto`), which are
tables in TOML, and `setopt -p history share ignore_space` sets several of a group at once (zsh's names still work).
Configuration also comes in layers, each able to override the one before: `config.toml`, then the plugins it enables
(a plugin can bundle options that go together, or your whole personal setup, to share between machines), then the
files in `rc.d`. A setting that is wrong is reported with its file and line, and skipped. See [Settings
in config.toml](usage.md#settings-in-configtoml).

## Newly installed commands are found

Shells remember where they found each command in `PATH`, so they don't have to search every directory each time
you run it. The `hash` built-in shows that list. In dash, bash and zsh, the list stays as it is until you change
`PATH` or run `hash -r` (`rehash` in zsh), even when the directories change on disk. Some cases that go wrong:

- **A new command shadows an old one.** You ran `python` (found in `/usr/bin`), then installed another one in
  `~/.local/bin`, which comes earlier in your `PATH`. The shell keeps running `/usr/bin/python`.
- **A command moves.** In bash, running a command that has since moved or been deleted fails with
  `bash: /usr/local/bin/tool: No such file or directory`, even if it is still in another `PATH` directory.
- **A profile is switched.** Nix puts a directory in `PATH` that is really a symlink to the current version of your
  profile. Switching profiles can add a command that shadows a remembered one, as above. Watching modification
  times doesn't catch this either, because every directory in the Nix store has the same one (1 January 1970).

luish checks the `PATH` directories after each command line you enter. If any of them changed (a file was added,
removed or renamed in it, or it is now a different directory), luish discards the whole list, so the next search
finds the right command. You never need `hash -r`.

```console
$ PATH=~/a:~/b:$PATH
$ tool                  # only in ~/b
from b
$ cp tool-v2 ~/a/tool   # (or installed from another terminal)
$ tool
from a
```

If a remembered command has been deleted, luish looks for it in the rest of `PATH` instead of failing, as dash does
(bash doesn't).

**Cost.** The check is one `stat` per `PATH` directory: about 20 system calls, which take a few microseconds in
total. It runs only in interactive shells, so scripts are unaffected. luish doesn't use inotify (which Linux limits
to 128 instances per user by default, shared with editors and other programs), and needs no background activity.

**Limits.** A change is noticed on the next command line. A command installed and then run on the same line (such
as `pip install --user tool; tool`) is still found by a normal search if it wasn't cached before; only a command
that shadows a cached one is missed until the next line.

The same check keeps Tab completion of command names up to date: new commands in `PATH` are offered as soon as they
are installed.
