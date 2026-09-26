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
