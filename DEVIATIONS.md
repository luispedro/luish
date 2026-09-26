# Deliberate differences from dash

luish treats dash as the reference implementation, but differs from it in
these places. Where luish goes beyond dash, zsh is the preferred model, since
luish is meant to replace it as a daily-driver shell; some older differences
follow bash instead.

Each difference has a test in `tests/cases/`: either a case marked
`# reference: zsh`, which is compared with `zsh --emulate sh` (pinned in
`pixi.toml`) instead of dash (words after `zsh` are further options for
it), or one with a `.expected` file.

## Following zsh

| Behaviour | dash | luish | Test |
|---|---|---|---|
| `source` | Not a built-in (`not found`, status 127) | As in zsh: `.`, but a name without `/` is looked for in the current directory before `PATH`, and further arguments are the positional parameters while the file runs. Special, as in zsh's sh emulation. A file that can't be read is an error with status 2, as for `.` (zsh uses 1) | `builtins/source.sh` (zsh), `builtins/source_missing.sh` |
| `pushd`, `popd`, `dirs` | Not built-ins (`not found`, status 127) | As in zsh with its default options: `+n` and `-n` name entries of the stack (zsh's sh emulation sets `POSIX_CD`, which makes them directory names). `popd` with an argument other than `+n` or `-n` is an error with status 1 (zsh usually does nothing, with status 0) | `builtins/dirstack.sh` (zsh `-o noposixcd`), `builtins/dirstack_interactive.sh` (zsh), `builtins/popd_dir.sh` |
| `setopt`, `unsetopt` | Not built-ins (`not found`, status 127) | As in zsh: set options by name (case and `_` don't matter, a `no` prefix inverts), dash's and luish's own (such as `promptpercent`). Without arguments they list the options that are on or off; zsh lists those that differ from their defaults | `options/setopt.sh` (zsh), `options/setopt_list.sh` |
| `%` sequences in prompts | Not supported | With `setopt promptpercent`, as in zsh (after parameter expansion, as with zsh's `PROMPT_SUBST`), with a subset of its sequences. The escape sequences for attributes and colours are ANSI SGR codes rather than the terminal's own (so turning bold off is `\e[22m`, where zsh writes `\e[0m` and restores the rest) | `misc/prompt_percent.sh` |
| `**/` | The same as `*/` | With `setopt globstar` (off by default), matches any number of directories, as in zsh (where it is always on) | `expand/globstar.sh` (zsh), `expand/globstar_off.sh` (dash), `expand/globstar_loop.sh` |
| Glob qualifiers, `*(/)` | A syntax error | With `setopt bareglobqual` (off by default), zsh's glob qualifiers. Subscripts count from 1, as in native zsh (its `sh` emulation sets `KSH_ARRAYS`, which makes them count from 0) | `expand/glob_qualifiers.sh`, `expand/glob_qualifier_errors.sh` (zsh `+o shglob -o bareglobqual +o ksharrays`), `builtins/internal_savestate_globqual.sh` |
| A directory as a command | `not found` (status 127) | With `setopt autocd` (off by default), changes to it when read from standard input, as in zsh | `builtins/autocd.sh` (zsh) |
| History file | None (Debian's dash has no line editor; upstream dash with libedit keeps the history in memory only) | In zsh's format, so the two shells can share a file: `: START:0;COMMAND`, metafied, with `\` before embedded newlines. New entries are appended on exit (zsh's `append_history`), or after each command with `setopt inc_append_history` or `share_history`, under an `fcntl` lock (zsh's `hist_fcntl_lock`). Unlike zsh: without `HISTFILE` the file is `$XDG_STATE_HOME/luish/history` (zsh saves nothing), `SAVEHIST` defaults to `HISTSIZE` (zsh: 0), `HISTFILE` and `HISTSIZE` set in the startup files take effect, a command equal to the previous one is never added (zsh's `hist_ignore_dups`, off there by default), and the elapsed time is always written as 0 | `history_file` and `share_history` in `tests/interactive.rs`, unit tests in `interactive/histfile.rs` |
| Last command of `sh -c` | Debian's dash forks it (a Debian patch; upstream dash execs it) | Replaces the shell with it unless a trap is set, as zsh, bash and upstream dash do | `exec/c_exec_last.sh` (zsh) |

## Following POSIX where dash doesn't

| Behaviour | dash | luish | Test |
|---|---|---|---|
| Script read from a pipe | Reads ahead in blocks, so commands in the script that read stdin miss data | Never reads past the current command (POSIX requirement), as zsh | `misc/stdin_script.sh` (zsh) |
| `$LINENO` | Not supported (empty) | Current line number, as in POSIX and bash | `misc/lineno.sh` |
| fd numbers in redirections | Only a single digit is an fd number: `exec 20>f` runs a command named `20`, and `echo hi 99>&1` prints `hi 99` | Any number of digits, as POSIX allows (and bash does; zsh is like dash) | `exec/redirect_big_fd.sh` |
| `exec -- cmd` | No `--` handling: tries to run a command named `--` (status 127) | `--` ends the options, as POSIX requires (and bash and zsh do) | `exec/exec_dashdash.sh` |
| `cd -e` | Not supported (`Illegal option -e`, status 2) | The POSIX 2024 option: with `-P`, status 1 if the directory is changed but its name can't be found (as bash does) | `builtins/cd_e.sh` |
| Options listed by `set -o` / `set +o` | The last one is `debug` (no option letter) | The last one is `hashall` (`-h`, which POSIX has and dash lacks); luish has no `debug` option | `options/set_o_hashall.sh` |

## Following bash

| Behaviour | dash | luish | Test |
|---|---|---|---|
| `set -x` output | Arguments printed unquoted | Arguments quoted so the trace can be re-read as input (as bash does) | `options/xtrace.sh` |
| `kill %n` for a job started without job control | Signals the process group `-pid`, which doesn't exist, and fails with "No such process" | Signals each process of the job (as bash does) | `builtins/kill_job.sh` |
| `fc` | Debian's dash has none (`fc: not found`, status 127). Upstream dash (with libedit) lists as `%5d cmd`, counts its own entry, and doesn't echo edited commands | The POSIX list format (`N\tcmd`, continuation lines indented by a tab). As in bash, its own entry is left out, and commands it re-runs replace that entry and are echoed to stderr. An event number outside the history is moved to the nearest end (dash moves a `first` that is too large to the oldest entry). Fails with status 2 in a non-interactive shell | `builtins/fc_noninteractive.sh`, and `fc_history` in `tests/interactive.rs` |
| `$((` that is not arithmetic | Syntax error (dash always reads `$((` as arithmetic) | Read as `$( (...) )`, a command substitution of a subshell, as bash does (POSIX leaves it unspecified) | `parse/arith_fallback.sh` |
| `emacs` option in interactive shells | Off (Debian's dash has no line editor) | On unless `vi` is set, since it is the line editor's mode, so `$-` has `E` (as in bash) | `options/interactive_c.sh` |

## luish's own

| Behaviour | dash | luish | Test |
|---|---|---|---|
| Command cache (`hash`) in an interactive shell | Kept until `PATH` is assigned or `hash -r`, so a command installed earlier in `PATH` than a cached one is ignored | Also cleared when a `PATH` directory changes (checked after each line is read) | `path_cache` in `tests/interactive.rs` |
| `__luish_internal` | Not a built-in (`not found`, status 127) | luish's own built-in, with subcommands such as `savestate`, which prints commands that restore the shell's state, and `print-git-rev` (see `STATUS.md`) | `builtins/internal_savestate.sh`, `builtins/internal_git_rev.sh` |
| Startup files in `$XDG_CONFIG_HOME/luish/rc.d/` and `login.d/` | Login shells read `/etc/profile` and `~/.profile`; interactive shells read `$ENV` | If `rc.d` exists, interactive shells first run its `*.lsh` files; if `login.d` exists, login shells then run its files instead of `/etc/profile` and `~/.profile`. Their effects are cached (see `STATUS.md`). Without the directories, as dash | `misc/startup_cache.sh` |
| `help` | Not a built-in (`not found`, status 127) | In interactive shells (and their subshells), a built-in that shows help for the built-ins. Not a built-in in scripts or `-c`, as in dash; there, `__luish_internal help` does the same | `builtins/internal_help.sh`, `builtins/help_noninteractive.sh` (same as dash), and `help_builtin` in `tests/interactive.rs` |
| `plugin` | Not a built-in (`not found`, status 127) | In interactive shells (and their subshells), a built-in that loads Rhai plugins (`plugin load`, `list`, `unload`; see `STATUS.md`). Not a built-in in scripts or `-c`, as in dash; there, `__luish_internal plugin` does the same | `builtins/internal_plugin.sh`, `builtins/plugin.sh` (same as dash), and `plugin_builtin` in `tests/interactive.rs` |
