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

Current state: **98 differential cases, 24 unit tests and 7 pty tests
pass**.

## Environment

- pixi provides `rust`. dash and bash come from the system,
  because conda-forge has no dash *shell* package (its `dash` package is
  Plotly Dash).
- Plugins will be written in Rhai (PLAN.md §6). No plugin code or dependency
  exists yet.

## Phase status

| Phase | Status |
|---|---|
| 0 Scaffolding | Done, except for a CI workflow file. `pixi run check` runs the CI steps locally. |
| 1 Minimal REPL / external commands | Done |
| 2 Lexer | Done, apart from the fuzz target |
| 3 Parser | Done. There are unit tests, but no `insta` snapshots or fuzz target |
| 4 Executor core | Done |
| 5 Word expansion | Done |
| 6 Variables and built-ins | Done |
| 7 Functions, `eval`, `.`, control flow | Done |
| 8 Signals and traps | Mostly done (see the gaps below) |
| 9 Options and `set -e` | Done |
| 10 Interactive / job control | Done (prompt loop, history and `fc`, job control, completion), with the gaps listed below |
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
- Quoting: single quotes, double quotes, backslash, and line continuation
  (also inside `$` expansions, as in `$\<newline>?`, as in dash).
- `${...}` in all its forms, `$((...))` (with a fallback to `$( (...) )`),
  and backquotes (unescaped, then parsed separately).
- Here-docs: `<<`, `<<-`, and quoted delimiters (no expansion). Several
  here-docs can be on one line, and here-docs work inside `$(...)`. Bodies
  larger than 64 KiB go through an unlinked temporary file.
- Aliases are expanded by splicing the alias text into the input buffer,
  with a recursion guard. An alias value ending in a blank also makes the
  next word eligible for alias expansion, wherever it is (also a `for`
  variable, `in`, or a `case` word, as in dash). Test:
  `parse/alias_blank_compound.sh`.
- Reserved words are recognised only in command position.
- Function definitions have the form `name() command`: as in dash, the body
  may be any command (`f() echo hi`, `f() g() { ...; }`).
- As in dash, a bad `${...}` (such as bash's `${x//a/b}`) is an error only
  when it is expanded, so it can sit in a branch that isn't taken, and `$(`
  in a here-doc delimiter is a syntax error. Test: `parse/dash_lenient.sh`.
- `$((` that isn't arithmetic is read as `$( (...) )` (a deviation: dash
  reports an error). Test: `parse/arith_fallback.sh`.
- Parsing is incremental: one line (list) is parsed and executed at a time.
  Incomplete input is reported as such, which triggers `PS2`.
- Syntax errors use dash's wording and report the line number.
- Tests: `parse/*`, plus unit tests in `parser.rs`.

### Expansion (`src/expand/`)
- Tilde expansion (`~`, `~user`, and after `:` in assignments, including
  in the word of `${x-word}` within an assignment).
- Every `ParamOp`, with and without `:`.
- A double-quoted part is always a field, even when it expands to nothing
  (`"$u"`, `"${u+x}"`), except a lone `"$@"` with no parameters.
- `"$@"` produces zero or more fields, while `"$*"` joins the parameters
  with the first character of IFS. In command words, `$@`, `$*` and `"$@"`
  give separate fields even when IFS is empty; elsewhere (assignments,
  `case` words) both are joined with the first character of IFS, as in
  dash.
- As in dash, `$@` and `$*` always count as set for `${@-x}` and `${@+x}`,
  and are null for `${@:-x}` when their joined length is zero (counting
  separators by dash's rules); `${#@}` is the joined length. Test:
  `expand/positional_ifs.sh`.
- `set -u` errors on unset variables, except for `$@` and `$*`.
- Field splitting happens as the text is built (`split.rs`) and follows the
  IFS rules for whitespace and non-whitespace characters. Empty IFS turns
  splitting off; unset IFS means the default.
- Arithmetic (`arith.rs`): all the C operators, including assignment
  operators, short-circuit evaluation, and decimal, octal and hex literals.
  Division by zero is an error. A variable holding only blanks is 0. As in
  dash, quotes and backslashes inside `$((...))` are kept in the text given
  to the evaluator, so they are errors. Test: `expand/arith_quotes.sh`.
- Command substitution strips trailing newlines and, as in dash, drops NUL
  bytes (so does `read`). It sets `$?` only for
  commands consisting of assignments alone, as dash does. Test:
  `expand/nul_bytes.sh`.
- Pathname expansion is sorted in byte order and needs an explicit leading
  `.`. A `.*` pattern matches `.` and `..` (as in dash). A lone `[` is not a
  pattern.
- The pattern matcher handles `*`, `?`, bracket expressions, `!`
  negation (as in dash, `^` is an ordinary character), ranges and
  `[:class:]`. Quoted characters are always literal.
- Tests: `expand/*`, plus unit tests in `split.rs`, `pattern.rs` and
  `arith.rs`.

### Execution (`src/exec/`)
- Command lookup order: special built-in, function, regular built-in, then
  `PATH`. As in dash, found commands are cached with the index of their
  `PATH` directory and used without checking the file; if it is gone, the
  directories after it are tried (dash's `shellexec`). `cd` drops entries
  from relative directories. Test: `exec/path_cache.sh`.
- As in dash, a function can't be named after a special built-in ("Bad
  function name").
- Order (XCU 2.9.1, as in dash): the words are expanded, then the
  redirections are made in the shell (for every kind of command; a forked
  external command inherits them), then the assignments are expanded, so
  `x=$(cat) <<EOF` reads the here-doc. The `set -x` trace goes to the
  stderr from before the redirections. A failed open makes the command
  fail with status 2 (exiting the shell for a special built-in); an
  expansion error in a redirection is fatal. Test:
  `exec/assign_redirect_order.sh`.
- Assignment scope: assignments before special built-ins persist (and,
  for `exec`, are exported to the command). Before
  functions, regular built-ins and external commands they are temporary
  and exported, and they are made in the shell (as in dash), so assigning
  to a read-only variable is an error of the shell.
- A simple external command in the foreground of a non-interactive shell
  without job control is started with `posix_spawn` (glibc uses
  `clone(CLONE_VM|CLONE_VFORK)`, so the page tables aren't copied), like
  dash's `vforkexec`. Exec errors are reported by the shell. Interactive shells, and
  shells doing job control, fork: the child has to take the terminal and
  reset the signals the shell ignores. Tests: `exec/spawn.sh`,
  `exec/readonly_assign.sh`.
- Pipelines fork every stage, and the last stage's status is the pipeline's
  status. `!` inverts the status.
- Async lists (`&`) set `$!`. In a non-interactive shell they ignore INT and
  QUIT and read stdin from `/dev/null`.
- Redirections are applied left to right. In-process commands save and
  restore the fds they change, keeping the copies at fd 10 or above with
  close-on-exec. `exec` without a command makes redirections permanent.
  `n>&n` does nothing even if `n` is closed, and `>&word` with a word that
  is not a number or `-` is a fatal syntax error, as in dash. Fd numbers
  may have several digits (a deviation: dash allows one). Tests:
  `exec/redirect_dup.sh`, `exec/redirect_big_fd.sh`.
- Exit statuses are 127 for not found (and, as in Debian's dash, for other
  exec errors such as `ELOOP`), 126 for not executable, and 128+N for
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
  left side of `&&`/`||`, and after `!`. As in dash, only simple commands,
  subshells and pipelines (and a compound command whose redirection
  fails) exit on their own status, so `{ false && true; }` does not exit.
  Test: `errexit/compound.sh`.
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
  `%+`, `%-`, `%str`, `%?str` (an ambiguous spec is an error). `kill` is a
  port of dash's (options, `-l`), and signal names follow dash's table
  (any case, no `SIG` prefix, `RTMIN+n`/`RTMAX-n`, no name for 16), also
  for `trap`, which, as in dash, takes no options. Test:
  `builtins/kill_trap_signals.sh`.
- Tests: `builtins/jobs.sh`, `builtins/kill_job.sh`, `exec/async_pid.sh`,
  `exec/exec_last.sh`, `exec/c_exec_last.sh`, `tests/interactive.rs`, and
  unit tests in `cmdtext.rs`.

### Built-ins (`src/builtins/`)
- Special: `:` `.` `break` `continue` `eval` `exec` `exit` `export`
  `local` (special in dash) `readonly` `return` `set` `shift` `times`
  `trap` `unset`.
- Regular: `[` `alias` `bg` `cd` `command` `echo` `false` `fc` `fg` `getopts`
  `hash` `jobs` `kill` `printf` `pwd` `read` `test` `true` `type`
  `ulimit` `umask` `unalias` `wait`.
- `echo` follows dash: `-n` only, and XSI escapes are always processed
  (with dash's octal forms `\0nnn` and `\nnn`, and Debian's `\e`).
- `printf` supports every conversion (numeric ones use libc `snprintf`),
  `%b`, `*` for width and precision, and reuses the format while arguments
  remain. As in dash, unsigned conversions parse with `strtoull` (so `-1`
  wraps), out-of-range numbers are reported with `strerror(ERANGE)`, and an
  invalid directive gives status 2. Test: `builtins/printf_escapes.sh`.
- `getopts` is a port of dash's: `OPTIND` moves past an argument as soon as
  its first letter is read, `OPTARG` is left alone at the end, and the
  position is reset by assigning `OPTIND`, by `set --` and `shift`, and is
  saved and restored across function calls. As in dash, `OPTIND` must be
  a number (assigning anything else, or unsetting it, is an error). Test:
  `builtins/getopts_dash.sh`.
- Numeric arguments (`exit`, `return`, `shift`, `kill`, `wait`) follow dash's
  `number`: decimal, blanks and a sign allowed, 0 to `INT_MAX`.
- As in dash's `evalbltin`, a built-in whose output can't be written prints
  `name: I/O error`, and its status gets bit 1. Test:
  `builtins/write_errors.sh`.
- `set -x` doesn't trace commands run while `PS4` is expanded (dash's
  `inps4`), so a command substitution in `PS4` doesn't loop. Test:
  `options/xtrace_ps4_subst.sh`.
- `test` is a port of dash's parser (the POSIX rules for three and four
  arguments, then recursive descent with dash's operand/operator
  disambiguation), so ambiguous expressions such as `[ -a -a ]` and the
  error messages match dash. Test: `builtins/test_parse.sh` (every
  expression of up to four arguments from a set of tokens).
- `export`, `readonly` and `local` (also through `command`, and when the
  name comes from an expansion) expand arguments that look like
  assignments as assignments, as dash 0.5.12 does: no field splitting or
  globbing, and tilde expansion after `=` and `:`. Test:
  `builtins/declaration_args.sh`.
- `command` stops errors in special built-ins from exiting the shell.
  `command -v`/`-V` and `type` follow dash's `describe_command` (only the
  first name for `command`, "not found" on stdout, "tracked alias" for
  cached commands, a path is found if the file exists), and `command -p`
  searches dash's default path. Test: `builtins/command_describe.sh`.
- `cd` and `pwd` use the logical directory kept by the shell (dash's
  `curdir`): `pwd` works after the directory is removed, `cd -` without
  `OLDPWD` is `cd .`, and `OLDPWD` is exported. At startup a valid `$PWD`
  is used without `getcwd`. Test: `builtins/cd_logical.sh`.
- `umask` and `ulimit` are ports of dash's (symbolic modes; `-H`/`-S`, `-a`
  format). Tests: `builtins/umask_modes.sh`, `builtins/ulimit_dash.sh`.
- `unset` of a bad name is an error; `set -` turns off `-x` and `-v` without
  resetting the parameters; `.` of a directory reads nothing. Test:
  `builtins/special_misc.sh`.
- `local` is scoped per function call. As in dash, `local x` keeps the
  current value.
- Tests: `builtins/*`.

### Options (`src/options.rs`)
- Letters `e f I i m n s x v V E C a b u p h` and the long names, listed in
  dash's table order. `$-` shows the letters in reverse table order, as dash
  does.
- `set -o` / `set +o` output matches dash, except that the last option is
  `hashall` rather than dash's `debug` (see DEVIATIONS.md).
- Tests: `options/*`.

### Interactive mode (`src/interactive/`)
- A rustyline editor with emacs mode by default and vi mode under
  `set -o vi`.
- History in `$HISTFILE`, limited to `$HISTSIZE` entries. Each entry is
  one top-level command as read (possibly several lines); a command equal
  to the newest entry is not added again. The store is luish's own
  (`src/interactive/history.rs`), in rustyline's file format (`#V2`, with
  `\` and newlines escaped, mode 0600); it gives entries event numbers that
  stay the same when old entries are dropped. The file is written on exit,
  only if the history changed.
- `fc` (POSIX; upstream dash has it with libedit, Debian's dash has none):
  `-l` lists (default: the last 16), `-n` omits numbers, `-r` reverses,
  `-s [old=new]` re-runs, and `-e editor` (default `$FCEDIT`, `$EDITOR`,
  then `ed`; `-e -` is `-s`) edits the commands in a temporary file and
  runs the result, unless the editor fails. `first` and `last` are event
  numbers (clamped to the history), negative offsets, or a prefix of a
  command. As in bash, the `fc` command's own entry is left out; when it
  re-runs commands they replace that entry and are echoed to stderr.
  Re-running `fc` is limited to 4 levels (dash's `MAXHISTLOOPS`). In a
  non-interactive shell `fc` fails with `history not active`.
- `PS1` and `PS2` go through parameter expansion.
- Ctrl-C cancels the current input.
- Tab completion (`src/interactive/complete.rs`), bash-style: the first Tab
  completes the common prefix, a second one lists the candidates. In
  command position (found by a rough tokenizer that follows quotes,
  operators, redirections, assignments, `$(`, backquotes, reserved words
  such as `then`, and commands such as `sudo` that take a command) it
  completes built-ins, reserved words, functions, aliases and executables
  in `PATH` (cached until `PATH` or a directory's mtime changes); a word
  with a `/` completes executables and directories. Elsewhere it completes
  filenames (with `~/`, and after `=` or `:` in an assignment or `=` in a
  `--option=`), and variable names after `$` or `${`. The text already
  typed is kept, and what is added is quoted for the quoting in effect at
  the cursor. Directories get a `/`, other unique matches a space (and the
  closing quote). Dot files are listed only for a prefix starting with `.`.
- The completer never sees `Shell`: before each prompt, the REPL gives it a
  snapshot of function, alias and variable names, `PATH` and `HOME`.
- Startup files: `/etc/profile` and `~/.profile` for login shells, then
  `$ENV`, then `$XDG_CONFIG_HOME/luish/luishrc`.
- Tests: `tests/interactive.rs` (job control, Ctrl-C and Ctrl-Z, terminal
  input and modes, completion, and `fc`), `builtins/fc_noninteractive.sh`,
  and unit tests in `complete.rs` and `history.rs`. The
  pty tests use `TERM=dumb`, under which rustyline does no editing, except
  `tab_completion`, which uses `TERM=vt100`.

## Known gaps

- History entries are UTF-8 strings (rustyline's), so invalid bytes in a
  command are replaced when it is recorded.
- `fc -e` runs the edited text as one history entry, rather than one entry
  per command.
- Completion has no `~user`, no programmable (per-command) completion, and
  skips filenames that are not valid UTF-8 (rustyline works on `String`s).
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

Release build, 2026-09-26, after starting external commands with
`posix_spawn`. Loops are best of 3; this machine was less loaded than for
earlier baselines, so compare only within a table.

| Benchmark | luish | dash |
|---|---|---|
| `-c true` (average of 200 runs) | 1.11 ms | 0.92 ms |
| `-c /bin/true` (average of 200 runs) | 1.77 ms | 1.73 ms |
| `while` loop, 100k `$((i+1))` iterations | 0.08 s | 0.08 s |
| Loop running `/bin/true` 3000 times | 1.81 s | 1.76 s |
| Loop running `x=$(echo hi)` 3000 times | 1.01 s | 1.06 s |

Before `posix_spawn`, the `/bin/true` loop took 2.42 s. The system dash
(Debian) forks for the last command of `-c`; luish execs it, like upstream
dash. The largest remaining gap is startup (`-c true`).
