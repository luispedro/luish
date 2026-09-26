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
- A script with the line `# reference: zsh` is compared with
  `zsh --emulate sh` (from pixi, pinned in `pixi.toml`) instead, for a
  deliberate deviation where luish follows zsh.
- A `NAME.expected` file (with an optional `NAME.status`) replaces the
  comparison, for a deliberate deviation that no reference shell matches.
  Record every deviation in DEVIATIONS.md.
- Each script runs in a fresh temporary directory (which is also `$HOME`),
  with `LC_ALL=C` and `$SH` set to the shell under test.
- Plugin cases (`tests/plugins/*.sh`, run only with the `plugins` feature)
  can't be compared with dash: each has a `NAME.expected`, and stderr must
  be empty or match `NAME.stderr` exactly.
- Interactive behaviour is tested in `tests/interactive.rs`, which runs
  `luish -i` on a pseudo-terminal (a small harness on `libc`, no extra
  crates). Each step waits for expected output, or for named processes to
  be in the terminal's foreground process group, never for a fixed time.

Current state: **121 differential cases, 15 plugin cases, 51 unit tests and 12 pty tests
pass**.

## Environment

- pixi provides `rust`. dash and bash come from the system,
  because conda-forge has no dash *shell* package (its `dash` package is
  Plotly Dash).
- Plugins are written in Rhai (PLAN.md §6), an optional dependency behind
  the `plugins` cargo feature (on by default). `cargo build
  --no-default-features` builds a shell without it.

## Phase status

| Phase | Status |
|---|---|
| 0 Scaffolding | Done. GitHub Actions (`.github/workflows/ci.yml`) runs `fmt-check`, `lint`, `test` and a release build on Ubuntu 24.04; `pixi run check` runs the same steps locally. |
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
| 11 Plugins | Started: the `plugin` built-in, the Rhai host with a small `sh` module, and the `chpwd` and `prompt` hooks (see Plugins below) |
| 12 Conformance / performance | Started: autoconf `configure` scripts and the Oils spec tests (see Conformance below), benchmark baseline |

## Implemented behaviour

### Command line (`src/main.rs`)
- `luish script [args]`, `luish -c cmd [arg0 [args]]`, `luish -s [args]`,
  and reading stdin when there are no operands.
- Option letters and `-o name` / `+o name`. `-i` forces interactive mode;
  with `-c` or a script it runs that (it reads stdin only without them).
  As in dash, `+c` works like `-c` and `-l` (or `+l`) makes a login shell.
  `--no-plugins` makes `plugin load` do nothing. Tests:
  `options/interactive_c.sh`, `tests/plugins/no-plugins.sh`.
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
  from relative directories. A search makes one `stat` per directory, and
  an `access` only for a regular file. The shell looks up an external command before
  forking for it, so that the cache lasts (and a missing command costs no
  fork); for a pipeline, it looks up each simple command whose name is a
  literal word, as dash (which uses `vfork`) remembers them. Tests:
  `exec/path_cache.sh`, `exec/hash_stale.sh` (also in forked processes),
  `exec/hash_temp_path.sh`, `builtins/hash_pipeline.sh`, and `path_cache`
  in `tests/interactive.rs`.
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
  Inside `$(...)` the suppression is reset (dash runs it with fresh
  flags). Tests: `errexit/compound.sh`, `errexit/cmdsubst_condition.sh`.
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
  `local` (special in dash) `readonly` `return` `set` `shift` `source`
  (not in POSIX or dash; as in zsh) `times` `trap` `unset`.
- Regular: `[` `alias` `bg` `cd` `chdir` (another name for `cd`, as in
  dash) `command` `dirs` (not in POSIX or dash; as in zsh, like `popd` and
  `pushd`) `echo` `false` `fc` `fg` `getopts` `hash` `jobs` `kill` `popd`
  `printf` `pushd` `pwd` `read` `setopt` and `unsetopt` (as in zsh) `test`
  `true` `type` `ulimit` `umask` `unalias` `wait`, and luish's own
  `__luish_internal`,
  and, only in interactive shells, `help` and `plugin` (see Plugins).
- `__luish_internal` (`src/builtins/internal.rs`) holds luish's own
  commands as subcommands, so that they don't take names from the command
  namespace (widely used ones may later get aliases). A missing or unknown
  subcommand is an error with status 2.
- `__luish_internal print-git-rev` and `print-git-rev-short` print the git
  revision luish was built from (the full or abbreviated hash, with `-dirty`
  if `src/`, `build.rs`, `Cargo.toml` or `Cargo.lock` differed from it, or
  `unknown` outside a git checkout). `build.rs` sets them at compile time,
  so they cost nothing at run time. Test: `builtins/internal_git_rev.sh`.
- `help` (`src/builtins/help.rs`) lists the built-ins with a one-line
  summary each, or shows the help for the names given (status 1 if one has
  none). The text is the Markdown in `docs/builtins/`, compiled in with
  `include_str!` and shown as plain text; the user documentation includes
  the same files (`docs/builtins.md`). It is a built-in only in shells
  started interactive (the `-i` option, which `set` can't change, so also
  in their subshells), so scripts find the same commands as in dash;
  `__luish_internal help` is the same command in any shell. Unit tests
  check that every built-in has a page, that the pages fit 80 columns and
  that the docs include them all. Tests: `builtins/help_noninteractive.sh`,
  `builtins/internal_help.sh`, `help_builtin` in `tests/interactive.rs`.
- `echo` follows dash: `-n` only, and XSI escapes are always processed
  (with dash's octal forms `\0nnn` and `\nnn`, and Debian's `\e`).
- `printf` supports every conversion (numeric ones use libc `snprintf`),
  `%b`, `*` for width and precision, and reuses the format while arguments
  remain. As in dash, unsigned conversions parse with `strtoull` (so `-1`
  wraps), out-of-range numbers are reported with `strerror(ERANGE)`, and an
  invalid directive gives status 2. Like dash's, it has no options. Test:
  `builtins/printf_escapes.sh`.
- Values in `set`, `export -p`, `readonly -p`, `alias` and `trap` output
  are quoted as dash's `single_quote` does (`'"'"'` for a quote). Test:
  `builtins/quoting_output.sh`.
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
- `cd` options are parsed like dash's (combined letters, the last of `-L`
  and `-P` wins, `Illegal option` with status 2). `cd -e` (POSIX 2024,
  not in dash) makes `cd -P` return 1 when the new directory's name can't
  be found (`getcwd` fails); without `-e` it returns 0, and either way
  `PWD` is then the logical path. Tests: `builtins/chdir.sh`,
  `builtins/cd_e.sh`.
- `umask` and `ulimit` are ports of dash's (symbolic modes; `-H`/`-S`, `-a`
  format). Tests: `builtins/umask_modes.sh`, `builtins/ulimit_dash.sh`.
- `source` is `.` as in zsh: a name without `/` is looked for in the
  current directory, then in `PATH`, and further arguments are the
  positional parameters while the file runs (restored afterwards). Tests:
  `builtins/source.sh` (compared with zsh), `builtins/source_missing.sh`.
- The directory stack (`src/builtins/dirstack.rs`, `Shell::dirstack`):
  `pushd`, `popd` and `dirs` follow zsh with its default options (so not
  its sh emulation, whose `POSIX_CD` makes `+n` and `-n` directory names).
  `pushd` changes directory through `cd`'s code (`CDPATH`, `PWD`, `OLDPWD`,
  the `chpwd` hook, but `-P` wins over `-L` as in zsh); without an operand
  it swaps the top two entries, or goes to `HOME` with an empty stack; `+n`
  and `-n` rotate. `popd` removes the top entry even if it can't change to
  it; `popd +n`/`-n` removes an entry without changing directory. Only
  `-q`, `-L` and `-P` are options (anything else, such as `-1`, is the
  operand). Interactive shells print the stack after `pushd` and `popd`
  unless `-q` is given. `dirs` prints with `~` for `HOME` (`-l`, `-p`, `-v`),
  clears (`-c`) or replaces the stack. Not implemented: zsh's `AUTO_PUSHD`,
  `PUSHD_*` and `DIRSTACKSIZE`, `cd +n`, and the `dirstack` array. Tests:
  `builtins/dirstack.sh`, `builtins/dirstack_interactive.sh` (both
  compared with zsh), `builtins/popd_dir.sh`.
- `unset` of a bad name is an error; `set -` turns off `-x` and `-v` without
  resetting the parameters; `.` of a directory reads nothing. Test:
  `builtins/special_misc.sh`.
- `local` is scoped per function call. As in dash, `local x` keeps the
  current value.
- `__luish_internal savestate` (`src/state.rs`) prints commands that restore the shell's
  state when run with `.`: the working directory and directory stack, `umask`, variables and
  their attributes (not `PPID` or `LINENO`), traps, functions, aliases,
  loaded plugins and options (not `-i`, `-s`, `-m` or `-n`). Functions are printed from the
  AST by `src/unparse.rs`, which keeps all quoting (unlike `cmdtext.rs`),
  so the text parses back to the same tree; command names in function
  bodies that are aliases are quoted, and a function named like an alias is
  preceded by `unalias`, so that reading the state back in a shell that has
  the aliases doesn't expand them. Restoring adds to the current state
  (nothing is unset or unexported), fails on a variable that is already
  readonly, and doesn't restore traps when run as `eval "$(__luish_internal savestate)"`
  (a command substitution resets them). Tests: `builtins/internal_savestate.sh`
  (a new shell reading the state prints the same state), and unit tests in
  `unparse.rs` (round trips through the parser).
- Tests: `builtins/*`.

### Options (`src/options.rs`)
- Letters `e f I i m n s x v V E C a b u p h` and the long names, listed in
  dash's table order. `$-` shows the letters in reverse table order, as dash
  does.
- `set -o` / `set +o` output matches dash, except that the last option is
  `hashall` rather than dash's `debug` (see DEVIATIONS.md).
- luish's own options (`EXTENDED` in `options.rs`, all off by default:
  `promptpercent`) have no letter and are not in
  `set -o` or `$-`, so `set` stays as in dash. They are set with `setopt`
  and `unsetopt` (not POSIX; as in zsh), which also set dash's options.
  Names are as in zsh: case and `_` don't matter, and a `no` prefix is
  added or removed to invert an option (`unsetopt glob` is `set -f`).
  Without arguments they list the options that are on or off, sorted. An
  unknown option, or `interactive` and `stdin`, is an error (status 1);
  the other names are still set. `savestate` restores luish's own options
  with `setopt` and `unsetopt`. Tests: `options/setopt.sh` (compared with
  zsh), `options/setopt_list.sh`, `builtins/internal_savestate.sh`.
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
- With `setopt promptpercent`, `PS1`, `PS2` and `PS4` then expand zsh's
  `%` sequences (`src/prompt.rs`), in all shells: the directory (`%~`,
  `%d`, `%c`, `%C`, with a number of components), user, host, `%#`, `%?`,
  history number, jobs, `$SHLVL`, line number, terminal, dates and times
  (`%D{...}` through `strftime`), bold, underline, standout, colours
  (`%F`, `%K`: names, 0 to 255, `#rrggbb`), `%E`, `%{...%}` and `%G`,
  conditionals `%(x.yes.no)` (conditions `? # ! g j L / C ~ . c T t d D
  w`) and truncation (`%N<...<`, `%N>...>`, up to the end of the
  enclosing group). Unknown sequences expand to nothing. Colours are
  written as ANSI SGR sequences, whatever the terminal. The expansion keeps
  escape sequences apart from the text, and the line editor is given both
  (rustyline's `(raw, styled)` prompt), so the cursor position doesn't
  count them. Nothing is done unless the option is on and the prompt has a
  `%`. Tests: `misc/prompt_percent.sh` (checked against zsh while
  written; zsh can't be the reference because its interactive mode writes
  more than the prompts), `prompt_percent` in `tests/interactive.rs`, and
  unit tests in `prompt.rs`.
- After each line is read, the shell `stat`s the `PATH` directories and
  clears the command cache if one changed (device, inode or modification
  time), so a newly installed command is found even if it shadows a cached
  one (see DEVIATIONS.md and `docs/improvements.md`).
- Ctrl-C cancels the current input.
- Tab completion (`src/interactive/complete.rs`), bash-style: the first Tab
  completes the common prefix, a second one lists the candidates. In
  command position (found by a rough tokenizer that follows quotes,
  operators, redirections, assignments, `$(`, backquotes, reserved words
  such as `then`, and commands such as `sudo` that take a command) it
  completes built-ins, reserved words, functions, aliases and executables
  in `PATH` (cached until `PATH` or one of its directories changes, judged
  as for the command cache); a word
  with a `/` completes executables and directories. Elsewhere it completes
  filenames (with `~/`, and after `=` or `:` in an assignment or `=` in a
  `--option=`), and variable names after `$` or `${`. The text already
  typed is kept, and what is added is quoted for the quoting in effect at
  the cursor. Directories get a `/`, other unique matches a space (and the
  closing quote). Dot files are listed only for a prefix starting with `.`.
- Syntax highlighting (`src/interactive/highlight.rs`), on by default:
  reserved words (in command position only), command names (in a
  different colour when they are not a built-in, function, alias or
  executable, except while the cursor is on them, since they may be
  unfinished), quoted strings, parameter and arithmetic expansions,
  `$(...)` and backquotes (whose contents are highlighted as commands),
  operators, redirections, here-document bodies, comments and the `NAME=` of
  assignments. A `PS2` line is highlighted in the context of the earlier
  lines, so an open quote or here-document carries over. `$LUISH_HIGHLIGHT`
  sets the colours as `class=SGR` entries separated by `:` (as in
  `GREP_COLORS`), over the defaults
  `keyword=1;34:command=32:unknown=1;31:string=33:var=36:subst=35:op=1:redir=1:comment=90:assign=34`;
  an empty SGR leaves a class uncoloured. `LUISH_HIGHLIGHT=none`, or a
  non-empty `$NO_COLOR`, turns it off. Both are read before each prompt.
  Command lookups are cached until the next prompt.
- The completer and the highlighter never see `Shell`: before each prompt,
  the REPL gives them a snapshot of function, alias and variable names,
  `PATH` and `HOME`, the colours, and the text of an incomplete command.
- Startup files: `/etc/profile` and `~/.profile` for login shells
  (interactive or not, as in dash), then, for interactive shells, `$ENV`,
  then `$XDG_CONFIG_HOME/luish/luishrc`.
- Cached startup files (`src/startcache.rs`, a first version of PLAN.md
  §9.2), each set enabled by its directory existing, as zsh's `.zshrc` and
  `.zlogin`: `$XDG_CONFIG_HOME/luish/rc.d/` for every interactive shell,
  then `$XDG_CONFIG_HOME/luish/login.d/` for login shells (instead of
  `/etc/profile` and `~/.profile`). So the order is `rc.d`, the login files
  (`login.d`, or else `/etc/profile` and `~/.profile`), `$ENV`, `luishrc`.
  The `*.lsh` files of a directory run in byte order (not dot files), and
  what they changed is saved to `$XDG_CACHE_HOME/luish/rc-HOST` or
  `login-HOST` (mode 0600, written through a rename; the directory gets a
  `CACHEDIR.TAG` and a `README` saying it can be removed): the difference
  between the state (`state.rs`) before and after, as commands
  (assignments, `unset`, function definitions, ...), so inherited
  variables that the files don't touch aren't saved. The key is the build
  of luish (the git revision, plus a hash of the sources for a dirty build;
  see `build.rs`), the directory, the list of files, and the fingerprint (device, inode, size,
  modification time) of each file and of every file they read with `.`. A
  later shell stats those files, reads the cache and runs the saved
  commands; if a fingerprint or the build differs, it reruns the files and
  rewrites the cache, without asking. A directory's `_uncached.lsh` runs every time,
  after its cached state. On the warm path this costs a stat per file, a
  directory read and one file read per directory. Test:
  `misc/startup_cache.sh`. Not yet done: keying on the inherited values the
  files read (so values such as `PATH=$HOME/bin:$PATH` keep the rest of
  `PATH` from when the cache was built, and an `rc.d` cache built in a
  login shell, before `login.d` ran, is used in shells started from it,
  which have the `login.d` variables), changes a fingerprint can't show
  (command output, files tested with `[`, a sourced file that didn't
  exist), background revalidation, and merging into running shells.
- Tests: `tests/interactive.rs` (job control, Ctrl-C and Ctrl-Z, terminal
  input and modes, completion, highlighting, `fc`, and the command cache),
  `builtins/fc_noninteractive.sh`, and unit tests in `complete.rs`,
  `highlight.rs` and `history.rs`. The pty tests use `TERM=dumb`, under
  which rustyline does no editing, except `tab_completion` and
  `syntax_highlighting`, which use `TERM=vt100`.

### Plugins (`src/plugins/`)
- Everything is behind the `plugins` cargo feature. Until the first
  `plugin load`, the only state is `Shell::plugins` (`None`) and the only
  cost is the `None` check in `cd`. Without the feature, `plugin load`
  fails with "luish was built without plugin support".
- `plugin` is a built-in only in interactive shells (and their
  subshells), like `help`; `__luish_internal plugin` is the same anywhere.
- `plugin load NAME|PATH...` loads `$XDG_CONFIG_HOME/luish/plugins/NAME.rhai`,
  or else the directory `NAME/` there (default `~/.config/luish/plugins`),
  or a path if the argument contains a `/`; loading a plugin again replaces
  it (its hooks move to the end).
- Directory plugins (PLAN.md §6.6): `plugin.rhai`, then `rc.lsh` (with
  `.`; not run if `plugin.rhai` fails; its status is ignored), and at least
  one of those or `login.lsh` (not run yet). `LUISH_PLUGIN_DIR` (absolute)
  and `LUISH_PLUGIN_NAME` are set while they run and restored after.
  `import` in Rhai loads `NAME.rhai` from the plugin's directory (the file's
  directory for a file plugin), for every module of the plugin; imported
  modules are cached until a plugin is loaded again. A plugin without
  `plugin.rhai` doesn't create the Rhai engine (the engine is created with
  the first Rhai code). `plugin unload` can't undo `rc.lsh`. Tests:
  `plugins/directory.sh`, `plugins/startup_cache.sh`.
  `plugin list` prints the names, `plugin unload NAME...` removes a
  plugin's hooks. Errors: status 1 (130 if interrupted); usage errors: 2.
- `savestate` prints `__luish_internal plugin restore NAME PATH` (absolute)
  for each loaded plugin, after aliases and before options, so the startup
  cache loads the plugins from `rc.d` again: `restore` loads a plugin under
  a name and runs its Rhai code, but not its `rc.lsh`. What a plugin's top
  level and `rc.lsh` changed is cached with the rest of the state, and the
  plugin's Rhai file and the files `rc.lsh` sources are fingerprinted.
- `plugins/rhai.rs`: one `Engine` (created on first load, with call-depth,
  expression-depth and size limits), one AST per plugin. Rhai's `print`
  and `debug` write lines to fds 1 and 2. SIGINT stops plugin code (checked
  in `on_progress`), leaving the signal pending for the shell. Rhai is
  built with `only_i64` (see PLAN.md §6.4); floats are available.
- `sh` module: `hook`, `getvar`, `setvar`, `export`, `unsetvar`, `cwd`,
  `plugin_dir`,
  `last_status`, `interactive`, `run` (shell code in the current shell;
  `exit` in it stops the plugin and exits the shell), `write` (fds 1 and 2).
- `fs` module (`plugins/fs.rs`), without forking: `exists`, `is_file`,
  `is_dir`, `is_link`, `kind` (lstat), `is_readable`, `is_writable`,
  `is_executable`, `size`, `mtime`, `newer` and `older` (nanoseconds; a
  missing file is older, as in make), `read_file`, `list_dir` (sorted),
  `readlink`, `find_up` (lexically up from `$PWD` or a directory). Missing
  files give `false` or `()`.
- `vcs` module (`plugins/vcs.rs`), git only, like zsh's `vcs_info`:
  `vcs::info([dir])` finds the repository lexically up from `$PWD` (a
  `.git` directory, or a `.git` file naming one, with `commondir` for
  worktrees) and reads `HEAD`, loose and packed refs, the action files
  (with `vcs_info`'s names) and the stash log without forking; it runs git
  only for the reftable format. `vcs::status([dir])` runs `git
  --no-optional-locks status --porcelain=v2 --branch -z` (forked like
  command substitution, stdin and stderr on `/dev/null`) and counts
  staged, unstaged, untracked and conflicted files, ahead and behind.
  Not supported: bare repositories, `GIT_DIR`, `GIT_CEILING_DIRECTORIES`.
- Hooks: `chpwd`, called with the old and new directory after each
  successful `cd`, `pushd` or `popd` (after `cd -` prints the directory, or
  `pushd` the stack), also in subshells.
  `$?` is kept; a failing hook is reported with the plugin's file and the
  others still run; a `chpwd` hook running `cd` doesn't re-trigger `chpwd`.
  `prompt`, called with no arguments before each `PS1` prompt (not `PS2`):
  the hooks are called from the most recently registered one until one
  returns a string, which is used instead of `PS1`, without parameter
  expansion but with `%` expansion under `promptpercent`. A hook that
  returns `()` leaves it to the earlier hooks, then `PS1`; one that fails
  or returns something else is reported and skipped. Each hook sees the
  `$?` of the last command, which is kept afterwards; `exit` in `sh::run`
  exits the shell.
- `plugins/bytes.rs`: non-UTF-8 bytes map to U+10FF80–U+10FFFF and back
  (PLAN.md §6.5); strings with NUL can't be set as variables.
- Tests: `tests/plugins/` (`chpwd`, `errors`, `exit`, `floats`, `fs`, `vcs`, `reload`,
  `recursion`, `interrupt`, `bytes`, `no-plugins`, `savestate`, `prompt`), unit
  tests for the byte conversion and `git status` parsing, `builtins/plugin.sh`,
  `builtins/internal_plugin.sh`, and `plugin_builtin` in
  `tests/interactive.rs`. CI also runs clippy and the tests
  with `--no-default-features`.

## Conformance

Checked on 2026-09-26 (the scripts are not in the repository; working-memory
notes how to rerun them):

- **autoconf**: GNU hello 2.12.1 and GNU sed 4.9 `configure` run under luish
  (as `CONFIG_SHELL`) with the same output as under dash and the same
  `config.h`; both build, and `make check` passes (sed: the same PASS/SKIP
  lists as the dash-configured tree). The sed `configure` takes the same
  time under both shells.
- **Oils spec tests** (`spec/*.test.sh` whose `compare_shells` include
  dash, 1620 cases), each run under dash and luish and compared on stdout
  and status: 161 differed at first, 43 now. The rest are the deviations
  in DEVIATIONS.md (`$LINENO`, `set -x` quoting, multi-digit fds, `exec --`,
  `kill` of jobs without job control, `$((` fallback), bash-only features
  (arrays, `shopt`, `printf -v`/`%q`, `declare`), cases that differ only by
  temporary directory names or timestamps, and the gaps below.

## Known gaps

- History entries are UTF-8 strings (rustyline's), so invalid bytes in a
  command are replaced when it is recorded.
- `fc -e` runs the edited text as one history entry, rather than one entry
  per command.
- The highlighter's tokenizer is approximate (like the completer's): it
  does not expand aliases, and a function or alias defined earlier on the
  same line is shown as unknown until the next prompt.
- Completion has no `~user`, no programmable (per-command) completion, and
  skips filenames that are not valid UTF-8 (rustyline works on `String`s).
- `set -b` (immediate job notification) is accepted but does nothing: jobs
  are reported only before a prompt. Job notifications are given only for
  input read a line at a time (interactive or stdin), not in scripts run
  with `set -m`.
- Plugins (Phase 11) support only the `chpwd` and `prompt` hooks, part
  of the `sh` module and the `fs` and `vcs` modules: no plugin built-ins,
  completers, other hooks, time budgets (so a slow `prompt` hook delays the
  prompt), `capture` or `parse_json`. The native built-ins have not
  been moved onto a `Builtin` trait (PLAN.md Phase 11, step 1). `import`
  in a plugin is not resolved relative to the plugin's directory.
- With the `plugins` feature, `-c true` starts about 250 µs slower than
  without it: loading `libm` (for Rhai's floats) and load-time relocations
  of Rhai's static data in the PIE executable. This is accepted while it
  stays under 1 ms.
- `trap` with no arguments, run inside a subshell or `$(...)`, doesn't show
  the parent's traps.
- `read` is not interrupted by trapped signals.
- `${@#pat}` and `${*%pat}` operate on the joined string rather than on each
  parameter.
- `set -v` output is approximate.
- Prompt expansion lacks zsh's `%_`, `%e`, `%I`, `%N`, `%x`, `%v`,
  `%[...]`, the `l`, `S`, `_`, `e` and `v` conditions, and widths relative
  to the terminal's (negative truncation lengths). luish doesn't maintain
  `SHLVL`, so `%L` shows the inherited value.
- Glob results are sorted in byte order; locale collation is not
  implemented.
- Fds saved at 10 or above could collide with a user redirection to fd 10+
  in the same command.
- `command local x=1` keeps the variable in the function; in dash it is
  local to the `command` invocation and disappears.
- Startup makes about 185 syscalls to dash's 85 (see the performance
  section).

## Performance baseline

Release build, 2026-09-26, after the Oils conformance fixes. Loops are best
of 3, on a loaded machine; the `/bin/true` loop varied between 1.8 and 2.0 s
for both shells, so compare only within a table.

| Benchmark | luish | dash |
|---|---|---|
| `-c true` (average of 200 runs) | 1.19 ms | 0.90 ms |
| `-c /bin/true` (average of 200 runs) | 1.88 ms | 1.78 ms |
| `while` loop, 100k `$((i+1))` iterations | 0.09 s | 0.09 s |
| Loop running `/bin/true` 3000 times | 1.80 s | 1.81 s |
| Loop running `x=$(echo hi)` 3000 times | 0.92 s | 0.90 s |

Per external command, luish now makes the same syscalls as dash (a cached
command is no longer checked with `stat`). The largest remaining gap is
startup (`-c true`): 185 syscalls to dash's 85. Most come from querying all
64 signal dispositions at startup (dash looks one up only when it first
changes it), the Rust runtime's initialisation (stdio poll, reading
`/proc/self/maps` for the stack guard, `sigaltstack`, SIGPIPE; avoidable
with `#![no_main]`), `libgcc_s` found through a `RUNPATH` into the pixi
environment plus a `libpthread` stub, `readlink /proc/self/exe` (could be
lazy) and HashMap seeding. Deferred for now by decision.

Before `posix_spawn`, the `/bin/true` loop took 2.42 s. The system dash
(Debian) forks for the last command of `-c`; luish execs it, like upstream
dash.
