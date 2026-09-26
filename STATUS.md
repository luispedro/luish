# luish — Implementation status

This file records what is implemented and how it is tested. Update it in
the same commit as any change in behaviour. The phases refer to
[PLAN.md](PLAN.md); deliberate differences from dash are listed in
[DEVIATIONS.md](DEVIATIONS.md).

## Checking for regressions

```sh
pixi run check     # cargo fmt --check, clippy -D warnings, cargo test
pixi run test      # tests only
LUISH_CASE=expand/ pixi run test   # only differential cases matching a substring
```

- **Every behaviour listed as done below must stay covered by a test.** When
  fixing a bug, add a case to `tests/cases/` first.
- Differential cases (`tests/cases/**/*.sh`) run under luish and under the
  system `dash`. stdout and the exit status must match exactly. For stderr,
  only whether it is empty is compared, unless the script contains the line
  `# stderr: exact`.
- A `NAME.expected` file (with an optional `NAME.status`) replaces the dash
  comparison. Use one only for a deliberate deviation, and record it in
  DEVIATIONS.md.
- Each script runs in a fresh temporary directory (which is also `$HOME`),
  with `LC_ALL=C` and `$SH` set to the shell under test.
- Interactive behaviour is tested in `tests/interactive.rs`, which runs
  `luish -i` on a pseudo-terminal (a small harness on `libc`, no extra
  crates). Each step waits for expected output, or for named processes to
  be in the terminal's foreground process group, never for a fixed time.

Current state: **64 differential cases, 18 unit tests and 5 pty tests
pass**.

## Environment

- pixi provides `rust` and `python`. dash and bash come from the system,
  because conda-forge has no dash *shell* package (its `dash` package is
  Plotly Dash).
- Cargo feature `python` enables the optional `pyo3` dependency. No plugin
  code exists yet.

## Phase status

| Phase | Status |
|---|---|
| 0 Scaffolding | Done, except for a CI workflow file. `pixi run check` runs the CI steps locally. |
| 1 Minimal REPL / external commands | Done |
| 2 Lexer | Done, apart from the fuzz target |
| 3 Parser | Done. There are unit tests, but no `insta` snapshots or fuzz target |
| 4 Executor core | Done |
| 5 Word expansion | Done |
| 6 Variables and built-ins | Done, except `fc` |
| 7 Functions, `eval`, `.`, control flow | Done |
| 8 Signals and traps | Mostly done (see the gaps below) |
| 9 Options and `set -e` | Done |
| 10 Interactive / job control | Partial: prompt loop and job control. No completion or `fc` |
| 11 Plugins | Not started |
| 12 Conformance / performance | Started: benchmark baseline below |

## Implemented behaviour

### Command line (`src/main.rs`)
- `luish script [args]`, `luish -c cmd [arg0 [args]]`, `luish -s [args]`,
  and reading stdin when there are no operands.
- Option letters and `-o name` / `+o name`. `-i` forces interactive mode.
  `--no-plugins` is accepted and currently does nothing.
- A missing script prints `cannot open X: No such file` and exits with
  status 127.
- A script without `#!` (execve returns ENOEXEC) is re-run with this
  executable.

### Parsing (`src/lexer.rs`, `src/parser.rs`)
- The lexer and parser share one `Parser` struct, so that `$(...)` can be
  parsed recursively (for example `case` patterns inside `$(...)`).
- Quoting: single quotes, double quotes, backslash, and line continuation.
- `${...}` in all its forms, `$((...))` (with a fallback to `$( (...) )`),
  and backquotes (unescaped, then parsed separately).
- Here-docs: `<<`, `<<-`, and quoted delimiters (no expansion). Several
  here-docs can be on one line, and here-docs work inside `$(...)`. Bodies
  larger than 64 KiB go through an unlinked temporary file.
- Aliases are expanded by splicing the alias text into the input buffer,
  with a recursion guard. An alias value ending in a blank also makes the
  next word eligible for alias expansion.
- Reserved words are recognised only in command position.
- Function definitions have the form `name() compound [redirs]`.
- Parsing is incremental: one line (list) is parsed and executed at a time.
  Incomplete input is reported as such, which triggers `PS2`.
- Syntax errors use dash's wording and report the line number.
- Tests: `parse/*`, plus unit tests in `parser.rs`.

### Expansion (`src/expand/`)
- Tilde expansion (`~`, `~user`, and after `:` in assignments).
- Every `ParamOp`, with and without `:`.
- `"$@"` produces zero or more fields, while `"$*"` joins the parameters
  with the first character of IFS.
- `set -u` errors on unset variables, except for `$@` and `$*`.
- Field splitting happens as the text is built (`split.rs`) and follows the
  IFS rules for whitespace and non-whitespace characters. Empty IFS turns
  splitting off; unset IFS means the default.
- Arithmetic (`arith.rs`): all the C operators, including assignment
  operators, short-circuit evaluation, and decimal, octal and hex literals.
  Division by zero is an error.
- Command substitution strips trailing newlines. It sets `$?` only for
  commands consisting of assignments alone, as dash does.
- Pathname expansion is sorted in byte order and needs an explicit leading
  `.`. A `.*` pattern matches `.` and `..` (as in dash). A lone `[` is not a
  pattern.
- The pattern matcher handles `*`, `?`, bracket expressions, `!`/`^`
  negation, ranges and `[:class:]`. Quoted characters are always literal.
- Tests: `expand/*`, plus unit tests in `split.rs`, `pattern.rs` and
  `arith.rs`.

### Execution (`src/exec/`)
- Command lookup order: special built-in, function, regular built-in, then
  `PATH` (with a cache).
- Assignment scope: assignments before special built-ins persist. Before
  functions and regular built-ins they are temporary and exported. Before
  external commands they are set in the child only.
- Pipelines fork every stage, and the last stage's status is the pipeline's
  status. `!` inverts the status.
- Async lists (`&`) set `$!`. In a non-interactive shell they ignore INT and
  QUIT and read stdin from `/dev/null`.
- Redirections are applied left to right. In-process commands save and
  restore the fds they change, keeping the copies at fd 10 or above with
  close-on-exec. `exec` without a command makes redirections permanent.
- Exit statuses are 127 for not found, 126 for not executable, and 128+N for
  death by signal N. Signal deaths other than INT and PIPE print a message.
- The last command of a forked child (subshell, background job, pipeline
  stage, command substitution) and of a `-c` string replaces the shell
  process instead of forking again, unless a trap is set (dash's `EV_EXIT`,
  carried as the `exit` flag of `run_list_exit` and friends). So `$!` is the
  command itself. A background pipeline forks its processes directly, and
  `$!` is its last process.
- While a signal is trapped, or the shell is interactive or doing job
  control, signals are blocked across `fork` until the child has reset its
  dispositions, so a signal sent to a new job is never lost.
- Control flow uses the `Flow` enum. `Flow::Error` means "exit if
  non-interactive, otherwise return to the prompt". It is used for syntax
  errors, expansion errors, and failures of special built-ins.
- `set -e` uses a counter that suppresses errexit inside conditions, on the
  left side of `&&`/`||`, and after `!`.
- Tests: `exec/*`, `errexit/*`.

### Jobs and job control (`src/jobs.rs`, `src/builtins/jobs.rs`)
- The job table follows dash: jobs are numbered by slot, and also kept in
  "current job" order (`%+` first, then `%-`). A finished job stays until it
  is reported by `jobs` or a notification. Without job control, a job whose
  status `wait` returned is freed when the next job is created.
- Without job control (non-interactive shells), only background jobs are
  recorded, with no command text (as in dash). Foreground commands are
  waited for directly.
- Job control is on by default in interactive shells (`-m`), and `set -m` /
  `set +m` switch it at run time. On startup the shell waits until it is in
  the foreground, puts itself in its own process group and takes the
  terminal; on exit it gives the terminal back.
- Under job control every job gets its own process group (the parent and
  the child both call `setpgid`), a foreground job gets the terminal, and
  waits use `WUNTRACED`. A stopped job is reported as
  `[1] + Stopped   cmd`. When a job is killed by SIGINT, the shell acts as
  though it got the SIGINT itself (as dash does).
- Terminal modes: after a foreground job exits normally the shell keeps the
  terminal's modes (so `stty` works); after a job stops or dies from a
  signal, the shell restores the modes it saved (as bash does; dash doesn't).
- Job text is rendered from the AST as dash's `cmdtxt` does
  (`src/cmdtext.rs`): `$x` becomes `${x}`, single quotes become double
  quotes, `$(...)` is elided, assignments are dropped.
- Changed jobs are reported on stderr before each prompt. `exit` or end of
  input with a stopped current job warns `You have stopped jobs.` once; an
  immediately repeated `exit` exits.
- Built-ins: `jobs [-l|-p] [job...]`, `fg`, `bg`, `wait [pid|job...]`
  (dash's statuses: 127 for an unknown pid, 2 for an unknown job; only the
  last pid of a pipeline names it), and `kill` with job specs `%n`, `%%`,
  `%+`, `%-`, `%str`, `%?str` (an ambiguous spec is an error).
- Tests: `builtins/jobs.sh`, `builtins/kill_job.sh`, `exec/async_pid.sh`,
  `exec/exec_last.sh`, `exec/c_exec_last.sh`, `tests/interactive.rs`, and
  unit tests in `cmdtext.rs`.

### Built-ins (`src/builtins/`)
- Special: `:` `.` `break` `continue` `eval` `exec` `exit` `export`
  `readonly` `return` `set` `shift` `times` `trap` `unset`.
- Regular: `[` `alias` `bg` `cd` `command` `echo` `false` `fg` `getopts`
  `hash` `jobs` `kill` `local` `printf` `pwd` `read` `test` `true` `type`
  `ulimit` `umask` `unalias` `wait`.
- `echo` follows dash: `-n` only, and XSI escapes are always processed.
- `printf` supports every conversion (numeric ones use libc `snprintf`),
  `%b`, `*` for width and precision, and reuses the format while arguments
  remain.
- `test` follows the POSIX rules for up to four arguments and uses
  recursive descent beyond that.
- `command` stops errors in special built-ins from exiting the shell.
- `local` is scoped per function call. As in dash, `local x` keeps the
  current value.
- Tests: `builtins/*`.

### Options (`src/options.rs`)
- Letters `e f I i m n s x v V E C a b u p h` and the long names, listed in
  dash's table order. `$-` shows the letters in reverse table order, as dash
  does.
- `set -o` / `set +o` output matches dash.
- Tests: `options/*`.

### Interactive mode (`src/interactive/`)
- A rustyline editor with emacs mode by default and vi mode under
  `set -o vi`.
- History in `$HISTFILE`, limited to `$HISTSIZE` entries.
- `PS1` and `PS2` go through parameter expansion.
- Ctrl-C cancels the current input.
- Startup files: `/etc/profile` and `~/.profile` for login shells, then
  `$ENV`, then `$XDG_CONFIG_HOME/luish/luishrc`.
- Tests: `tests/interactive.rs` (job control, Ctrl-C and Ctrl-Z, terminal
  input and modes). The line editor itself is not tested: the pty tests use
  `TERM=dumb`, under which rustyline does no editing.

## Known gaps

- No completion, and no `fc`.
- `set -b` (immediate job notification) is accepted but does nothing: jobs
  are reported only before a prompt. Job notifications are given only for
  input read a line at a time (interactive or stdin), not in scripts run
  with `set -m`.
- No plugin system (Phase 11).
- `trap` with no arguments, run inside a subshell or `$(...)`, doesn't show
  the parent's traps.
- `read` is not interrupted by trapped signals.
- `${@#pat}` and `${*%pat}` operate on the joined string rather than on each
  parameter.
- `set -v` output is approximate.
- Glob results are sorted in byte order; locale collation is not
  implemented.
- Fds saved at 10 or above could collide with a user redirection to fd 10+
  in the same command.

## Performance baseline

Release build, 2026-09-26, after job control (best of 3):

| Benchmark | luish | dash |
|---|---|---|
| `-c true` (average of 200 runs) | 1.85 ms | 1.67 ms |
| `-c /bin/true` (average of 200 runs) | 3.00 ms | 3.08 ms |
| `while` loop, 100k `$((i+1))` iterations | 0.077 s | 0.081 s |
| Loop running `/bin/true` 3000 times | 4.8 s | 3.4 s |
| Loop running `x=$(echo hi)` 3000 times | 1.26 s | 1.16 s |

The system dash (Debian) forks for the last command of `-c`; luish execs
it, like upstream dash. External commands are the largest gap: dash starts
them with `vfork`, luish with `fork`.
