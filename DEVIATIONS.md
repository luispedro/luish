# Deliberate differences from dash

luish treats dash as the reference implementation, but differs from it in
these places. Each one has a test with a `.expected` file in `tests/cases/`.

| Behaviour | dash | luish | Test |
|---|---|---|---|
| `$LINENO` | Not supported (empty) | Current line number, as in bash and POSIX | `misc/lineno.sh` |
| Script read from a pipe | Reads ahead in blocks, so commands in the script that read stdin miss data | Never reads past the current command (POSIX requirement) | `misc/stdin_script.sh` |
| `set -x` output | Arguments printed unquoted | Arguments quoted so the trace can be re-read as input (as bash does) | `options/xtrace.sh` |
| `kill %n` for a job started without job control | Signals the process group `-pid`, which doesn't exist, and fails with "No such process" | Signals each process of the job (as bash does) | `builtins/kill_job.sh` |
| Last command of `sh -c` | Debian's dash forks it (a Debian patch; upstream dash execs it) | Replaces the shell with it unless a trap is set, as upstream dash, bash and zsh do | `exec/c_exec_last.sh` |
| `fc` | Debian's dash has none (`fc: not found`, status 127). Upstream dash (with libedit) lists as `%5d cmd`, counts its own entry, and doesn't echo edited commands | The POSIX list format (`N\tcmd`, continuation lines indented by a tab). As in bash, its own entry is left out, and commands it re-runs replace that entry and are echoed to stderr. An event number outside the history is moved to the nearest end (dash moves a `first` that is too large to the oldest entry). Fails with status 2 in a non-interactive shell | `builtins/fc_noninteractive.sh`, and `fc_history` in `tests/interactive.rs` |
| Options listed by `set -o` / `set +o` | The last one is `debug` (no option letter) | The last one is `hashall` (`-h`, which POSIX has and dash lacks); luish has no `debug` option | `options/set_o_hashall.sh` |
| fd numbers in redirections | Only a single digit is an fd number: `exec 20>f` runs a command named `20`, and `echo hi 99>&1` prints `hi 99` | Any number of digits, as POSIX allows (and bash and zsh do) | `exec/redirect_big_fd.sh` |
| `exec -- cmd` | No `--` handling: tries to run a command named `--` (status 127) | `--` ends the options, as POSIX requires (and bash does) | `exec/exec_dashdash.sh` |
| `$((` that is not arithmetic | Syntax error (dash always reads `$((` as arithmetic) | Read as `$( (...) )`, a command substitution of a subshell, as bash does (POSIX leaves it unspecified) | `parse/arith_fallback.sh` |
| `emacs` option in interactive shells | Off (Debian's dash has no line editor) | On unless `vi` is set, since it is the line editor's mode, so `$-` has `E` (as in bash) | `options/interactive_c.sh` |
| Command cache (`hash`) in an interactive shell | Kept until `PATH` is assigned or `hash -r`, so a command installed earlier in `PATH` than a cached one is ignored | Also cleared when a `PATH` directory changes (checked after each line is read) | `path_cache` in `tests/interactive.rs` |
| `__luish_internal` | Not a built-in (`not found`, status 127) | luish's own built-in, with subcommands such as `savestate`, which prints commands that restore the shell's state, and `print-git-rev` (see `STATUS.md`) | `builtins/internal_savestate.sh`, `builtins/internal_git_rev.sh` |
| Startup files in `$XDG_CONFIG_HOME/luish/rc.d/` and `login.d/` | Login shells read `/etc/profile` and `~/.profile`; interactive shells read `$ENV` | If `rc.d` exists, interactive shells first run its `*.lsh` files; if `login.d` exists, login shells then run its files instead of `/etc/profile` and `~/.profile`. Their effects are cached (see `STATUS.md`). Without the directories, as dash | `misc/startup_cache.sh` |
| `help` | Not a built-in (`not found`, status 127) | In interactive shells (and their subshells), a built-in that shows help for the built-ins. Not a built-in in scripts or `-c`, as in dash; there, `__luish_internal help` does the same | `builtins/internal_help.sh`, `builtins/help_noninteractive.sh` (same as dash), and `help_builtin` in `tests/interactive.rs` |
