# luish, dash, bash and zsh

This page compares luish with the three shells people most often choose between on Linux: dash (the usual `/bin/sh`
on Debian and Ubuntu), bash and zsh. It describes luish 0.3.0 (October 2026), a young project that already
replaces dash for scripts, and is meant to replace zsh as an interactive shell; [](../compatibility.md) has the details
behind each claim here. Two further pages go into detail:
[](language.md), on arrays, expansions, arithmetic, globbing and the rest of the language, and
[](builtin-commands.md), on every built-in command of the four shells.

## In short

- **dash** is small, fast and POSIX, and has almost nothing for interactive use.
- **bash** is everywhere, and has the most scripting extensions, but runs scripts several times slower than dash.
- **zsh** has the richest interactive features and a vast completion system, but is also slow on scripts, and a
  typical setup (oh-my-zsh, conda, nvm) takes a noticeable time to start.
- **luish** aims at dash's speed and POSIX behaviour, zsh's interactive features, and the most used of bash's and
  zsh's scripting extensions, with some ideas of its own: a plugin manager, settings in TOML, and a cache that makes
  heavy startup files cost nothing.

## At a glance

| | dash | bash | zsh | luish |
|---|---|---|---|---|
| POSIX `sh` | Yes | With `--posix` | With `--emulate sh` | Yes, matching dash where POSIX is ambiguous |
| Speed of scripts (relative to dash) | 1× | 1.3 to 4.8× slower | 1.3 to 4.4× slower | 0.7 to 1.1× |
| Arrays and associative arrays | No | Yes | Yes | Yes (indices from 0, as bash and zsh's `sh` mode) |
| `[[ ... ]]`, `${x/a/b}`, `${x:1:2}`, `typeset` | No | Yes | Yes | Yes |
| Process substitution `<(...)` | No | Yes | Yes | Yes |
| `set -o pipefail` | No | Yes | Yes | Yes |
| Brace expansion, `$'...'`, `<<<` | No | Yes | Yes | Not yet |
| Line editing | None (Debian's) | readline | zle | emacs keys as zsh's, with `bindkey`; vi mode |
| Syntax highlighting, autosuggestions | No | No | With plugins | Built in |
| Tab completion | No | With bash-completion | compsys, thousands of commands | About 230 commands, git, a menu, and bash-completion as a fallback |
| History shared between sessions | No | Partly | Yes | Yes, in zsh's file format |
| Prompt `%` sequences | No | No (its own `\` escapes) | Yes | Most of zsh's, opt-in |
| `**/` and glob qualifiers | No | `**` with `globstar` | Yes | Yes, opt-in |
| Plugin manager | No | No (third-party) | No (third-party: oh-my-zsh, zplug ...) | Built in, from git, with a lock file |
| Configuration | Shell script | Shell script | Shell script | `config.toml`, plus shell scripts |
| Cached startup files | No | No | No | Yes |
| Platforms | Unix | Unix, Windows (via Cygwin and WSL) | Unix | Linux only |

## Running scripts

### Compatibility

luish implements the POSIX Shell Command Language and its required built-ins, plus `local`, and takes dash as its
reference. It is tested differentially: hundreds of test scripts run under both luish and dash, and must print the
same output and exit with the same status. Where luish deliberately differs, the test compares it with zsh instead.
Beyond its own tests, the `configure` scripts of GNU hello and GNU sed produce the same results under luish as under
dash, and of the 1620 cases of the [Oils](https://oils.pub) spec tests that are meant for dash, the few dozen
that differ are mostly deliberate differences or features of bash.

The deliberate differences are small, and mostly follow POSIX where dash doesn't: a script read from a pipe is never
read past the current command (so commands in it can read the rest of standard input), `$LINENO` works, and file
descriptors can have more than one digit (`exec 20>file`). See [](../compatibility.md) for all of them.

### Speed

On script benchmarks of a few hundred lines each (see [](../performance.md)), luish is as fast as dash, and faster on
arithmetic and function calls. bash and zsh take between 1.3 times as long (scripts that mostly start other programs,
such as `configure`) and 5 times as long (scripts that mostly run inside the shell):

| Benchmark | dash | luish | bash | zsh |
|---|---|---|---|---|
| arith | 1.00 | 0.65 | 2.99 | 1.65 |
| functions | 1.00 | 0.88 | 4.80 | 4.37 |
| strings | 1.00 | 0.97 | 3.54 | 2.78 |
| textproc | 1.00 | 1.03 | 2.49 | 3.30 |
| configure | 1.00 | 1.09 | 1.45 | 1.37 |
| build | 1.00 | 1.13 | 1.27 | 1.29 |
| arrays (not POSIX, so relative to luish) | | 1.00 | 4.95 | 5.08 |

Starting luish takes about 0.2 ms longer than dash (0.56 ms against 0.36 ms for `sh -c true`), and a command
substitution about 1.2 times as long, as it is a larger program; that is most of the 10% on the benchmarks that start
many programs. Everything luish adds beyond POSIX is opt-in or gives a meaning to what is a syntax error in dash, so
POSIX scripts don't pay for it.

### bash and zsh scripts

A script that needs bash or zsh only for arrays, `[[ ... ]]` and pattern substitution can often run under luish
unchanged, about five times faster. luish has:

- Indexed and associative arrays, `${!a[@]}`, `read -a`/`read -A`, and `typeset`/`declare` with the common options.
- `[[ ... ]]`, with `=~` (setting both bash's `BASH_REMATCH` and zsh's `match`), `function`, `let`, `source`,
  `builtin`.
- `${x:offset:length}`, `${x/pattern/replacement}`, bash's `${!name}` indirection and zsh's parameter flags such as
  `${(j:,:)a[@]}` and `${(o)a[@]}`.
- Process substitution, `pipefail`, and zsh's special variables (`RANDOM`, `SECONDS`, `EPOCHREALTIME`,
  `pipestatus`/`PIPESTATUS`, `path` ...).

Not yet: brace expansion (`{a,b}`, `{1..10}`), `$'...'` strings, here-strings (`<<<`), `select`, `coproc`, `shopt`
and `extglob`, `mapfile`, `printf -v`, `local -n`, `${x^^}`, `wait -n`, the `ERR` trap, the `time` keyword, and
arithmetic beyond dash's (`((...))`, `**`, `++`, floating point). Where bash and zsh disagree, luish documents which it follows (usually zsh's `sh`
emulation, sometimes bash).

[](language.md) compares these features one by one.

## Interactive use

luish is usable as a daily shell, and its author is moving to it from zsh. What it has:

- **Line editing** with zsh's emacs keys and widget names (`bindkey`), prefix search on Up and Down, `Alt-.`,
  `Ctrl-O`, `WORDCHARS`, and vi mode.
- **Syntax highlighting and autosuggestions** built in, as zsh's popular plugins provide them.
- **History** in zsh's file format, so the two shells can share `~/.histfile` during a move, shared between
  sessions, with history expansion (`!!`, `!$`, `^old^new`) and `fc`.
- **Prompts** with zsh's `%` sequences, also under long names (`%[fg:blue]%[dir]`), and a right prompt
  (`RPROMPT`).
- **Tab completion** with a menu, from the standard `completion` plugin: about 230 commands (coreutils, git, ssh
  hosts, package managers, cargo, npm, systemctl ...), and programs that complete themselves (Cobra programs such as
  `gh`, `docker` and `kubectl`, Click programs, nix). bash-completion can serve as a fallback for the rest.
- **zsh's conveniences**: `autocd`, `auto_pushd` and the directory stack, `CDPATH`, global and suffix aliases, `**/`,
  glob qualifiers (`vi *(.om[0])`), `print`, `setopt` with zsh's option names.
- **Job control** as in dash and bash: `jobs`, `fg`, `bg`, Ctrl-Z.

Still missing compared with a full zsh setup: compsys's breadth of completions and its configuration (`zstyle`),
grouped and coloured completion menus, user-defined widgets (`zle -N`), shell
functions as `precmd`/`preexec` hooks, and zsh's modules. Compared with bash, luish lacks readline's `inputrc`
configuration and the extensions listed above. dash has no line editing in Debian and Ubuntu, so any of these is an
improvement on it.

## What none of them have

- **Cached startup files.** luish saves the *effect* of the startup files (variables, functions, aliases, options)
  and restores it in new shells, so a shell that sets up `nvm` starts in about 8 ms however long the setup takes.
  Each file is cached on its own and rerun when it changes, and `__luish_cache` blocks cache part of a file, keyed
  on the variables and files it lists. See [](../usage.md#cached-startup-files).
- **A built-in plugin manager.** Plugins are listed in `config.toml`, fetched from GitHub or other git repositories,
  and pinned to exact commits in a lock file, so a configuration is reproducible across machines. A plugin can carry
  options, aliases, key bindings and an extension in [Rhai](https://rhai.rs), which runs inside the shell without
  starting a process (see [](../plugins.md)).
- **Settings in TOML**, with options named in groups (`history.share`, `glob.star`); zsh's names still work.
- **Commands installed while the shell runs are found.** luish notices when a `PATH` directory changes, so a new
  program that shadows an old one is used without `hash -r` or `rehash` (see [](../improvements.md)).

## Which to use

- For `/bin/sh` and portable scripts, dash remains the safe choice: it is mature and everywhere. luish can run the
  same scripts as fast, and is a good fit where scripts also want arrays or `[[ ... ]]` without bash's cost.
- For scripts written for bash, bash remains the reference; luish runs many of them, but not those that use the
  features above that it lacks.
- For interactive use, luish covers what a typical zsh setup does (prompt, highlighting, suggestions, shared history,
  completion of common commands) with less configuration and a faster start. zsh is still ahead on completion for
  less common programs, and on deep customization of the line editor.
- luish runs only on Linux.

## Where luish is going

The current work is finishing the replacement of zsh: more completion (a generic bridge to programs' own completion
and to `--help`), typing to narrow the menu, and `precmd`/`preexec` hooks. After that come terminal features (OSC 7
and OSC 133), error messages with a stack, a history with the directory, exit status and duration of each command,
and a mode for SSH in which the line editor runs on the local machine.

```{toctree}
:hidden:

language
builtin-commands
```
