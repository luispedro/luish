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
