# Deliberate differences from dash

luish treats dash as the reference implementation, but differs from it in
these places. Each one has a test with a `.expected` file in `tests/cases/`.

| Behaviour | dash | luish | Test |
|---|---|---|---|
| `$LINENO` | Not supported (empty) | Current line number, as in bash and POSIX | `misc/lineno.sh` |
| Script read from a pipe | Reads ahead in blocks, so commands in the script that read stdin miss data | Never reads past the current command (POSIX requirement) | `misc/stdin_script.sh` |
| `set -x` output | Arguments printed unquoted | Arguments quoted so the trace can be re-read as input (as bash does) | `options/xtrace.sh` |
