# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

luish is a POSIX `sh` for Linux, written in Rust, meant eventually to replace zsh as a daily-driver shell. dash is the
reference implementation: where POSIX is ambiguous, match dash. It must be as fast as dash, and anything beyond POSIX
must be opt-in and cost nothing when unused.

## Project documents

- `GOALS.md`: the goals, grouped into stages. When goals conflict, the earlier stage wins. Stage 1 (POSIX, dash speed,
  interactive daily driver) is the current focus.
- `PLAN.md`: the implementation plan, by phase (the phase numbers are used throughout the other documents).
- `STATUS.md`: what is implemented, how it is tested, and the known gaps. **Update it in the same commit as any
  change in behaviour.**
- `DEVIATIONS.md`: deliberate differences from dash. Each needs a `.expected` test.

## Commands

pixi coordinates everything. Run cargo through the pixi tasks or as `pixi run cargo ...`. dash and bash come from the
system (`/usr/bin`), not pixi: the conda-forge `dash` package is Plotly Dash, not the shell.

```sh
pixi run check                    # cargo fmt --check, clippy --all-targets -D warnings, cargo test (must stay green)
pixi run test                     # tests only
LUISH_CASE=expand/ pixi run test  # only differential cases whose path contains the substring
pixi run cargo test --bin luish parser::   # unit tests in one module
pixi run luish -c 'echo hi'       # run the shell
pixi run release                  # release build (LTO, panic=abort), used for benchmarks
```

`rustfmt.toml` sets `max_width = 120`. The `python` cargo feature enables the optional `pyo3` dependency; no plugin
code exists yet.

## Tests

Most coverage is differential (`tests/compare.rs`): each `tests/cases/**/*.sh` runs under luish and under dash.

- stdout and the exit status must match exactly. For stderr only emptiness is compared, unless the script contains
  the line `# stderr: exact`.
- `NAME.expected` (with optional `NAME.status`) replaces the dash run. Use it only for a deliberate deviation, and
  record the deviation in `DEVIATIONS.md`. `NAME.stdin` is fed to standard input.
- Each script runs in a fresh temporary directory that is also `$HOME`, with a cleared environment, `LC_ALL=C`, and
  `$SH` set to the shell under test.
- When fixing a bug, add a case first. Every behaviour listed as done in `STATUS.md` must stay covered.

## Architecture

All data (arguments, variables, filenames) is bytes (`Vec<u8>`), never UTF-8 strings. Process creation uses raw
`fork`/`execve` (via `src/sys.rs`, whose wrappers retry on EINTR), not `std::process::Command`, because subshells fork
without exec.

Pipeline: `input.rs` → `lexer.rs`/`parser.rs` → `ast.rs` → `exec/` (which calls `expand/` and `builtins/`), with all
state on the `Shell` struct in `shell.rs`.

- **Parsing is incremental.** One list (line) is parsed and executed at a time, so aliases and `set` affect later
  lines. The lexer and parser share a single `Parser` struct so that `$(...)` can be parsed recursively. For
  incremental input (stdin, interactive), a fresh `Parser` is created per attempt over the accumulated buffer, and
  incomplete input triggers `PS2`; `-c` and script files use one persistent parser. Aliases are expanded by splicing
  text into the parser's input (`splice_delta` corrects `consumed()`).
- **Reading stdin never reads past the current command** (POSIX; dash differs), so commands in a piped script can
  read the rest of stdin.
- **Control flow** is `ExecResult = Result<i32, Flow>`. `Flow::Error` means "exit if non-interactive, otherwise return
  to the prompt" and is used for syntax errors, expansion errors, and failures of special built-ins. `set -e` is a
  counter (`errexit_suppressed`) raised inside conditions, on the left of `&&`/`||`, and after `!`.
- **Command lookup** (`exec/simple.rs`): special built-in, function, regular built-in, then `PATH` (cached). Built-ins
  are a `fn` table in `builtins/mod.rs`, flagged special or regular; the flag decides assignment scope and whether
  errors exit the shell.
- **Forking**: `no_fork = true` is passed only for a pipeline stage's command (and a `( )` subshell already in a
  forked child); only then does an external command exec without forking again. Children reset traps and signal
  dispositions in `exec/fork.rs::child_reset`.
- **Redirections** for in-process commands save the original fds at fd ≥ 10 (close-on-exec) and restore them
  afterwards; `exec` without a command makes them permanent.
- **Expansion** (`expand/`): field splitting happens while the text is built (`split.rs`); `pattern.rs` is the
  matcher shared by globbing, `case`, and `${x#pat}`; `arith.rs` is the `$((...))` evaluator.
- **Signals** are installed without `SA_RESTART` so `wait` gets EINTR. Rust ignores SIGPIPE before `main`, so `main`
  restores the default.
- **Interactive mode** (`interactive/`) uses rustyline, kept behind its own module so the line editor stays separate
  from the executor (a Stage 3 SSH mode depends on this). Job control is minimal (`jobs.rs`).

## Conventions

- Error messages use dash's wording.
- Performance matters: hot paths (e.g. `[ ... ]` in loops) must avoid needless syscalls. Compare against dash with a
  release build when changing the executor or expansion.
- Documentation commits are prefixed `DOC`.
