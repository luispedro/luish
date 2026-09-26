# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

luish is a POSIX `sh` for Linux, written in Rust, meant eventually to replace zsh as a daily-driver shell. dash is the
reference implementation: where POSIX is ambiguous, match dash. It must be as fast as dash, and anything beyond POSIX
must be opt-in and cost nothing when unused. That is strict for scripts and `-c` (time and memory). Interactive shells
are judged on responsiveness and functionality instead: a few bytes per variable or history entry for interactive
features is fine, as long as the prompt and line editor stay fast.

## Project documents

- `GOALS.md`: the goals, grouped into stages. When goals conflict, the earlier stage wins. Stage 1 (POSIX, dash speed,
  interactive daily driver) is the current focus.
- `PLAN.md`: the implementation plan, by phase (the phase numbers are used throughout the other documents).
- `STATUS.md`: what is implemented, how it is tested, and the known gaps. **Update it in the same commit as any
  change in behaviour.**
- `DEVIATIONS.md`: deliberate differences from dash. Each needs a `.expected` test.
- `docs/`: user-facing documentation (Sphinx with MyST Markdown), published on Read the Docs via
  `.readthedocs.yaml`. Its Python dependencies are in both `docs/requirements.txt` (for Read the Docs) and the
  `docs` feature in `pixi.toml`; keep them in step.
- `docs/builtins/`: one Markdown page per built-in, which `help` shows (compiled in, see `src/builtins/help.rs`)
  and `docs/builtins.md` includes. Keep them up to date with the built-ins' behaviour.

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
pixi run docs                     # build the user docs into docs/_build/html (warnings are errors)
```

`rustfmt.toml` sets `max_width = 120`. Plugins (Phase 11) will use Rhai behind a `plugins` cargo feature (PLAN.md
§6); no plugin code exists yet.

## Tests

Most coverage is differential (`tests/compare.rs`): each `tests/cases/**/*.sh` runs under luish and under dash.

- stdout and the exit status must match exactly. For stderr only emptiness is compared, unless the script contains
  the line `# stderr: exact`.
- `NAME.expected` (with optional `NAME.status`) replaces the dash run. Use it only for a deliberate deviation, and
  record the deviation in `DEVIATIONS.md`. `NAME.stdin` is fed to standard input.
- Each script runs in a fresh temporary directory that is also `$HOME`, with a cleared environment, `LC_ALL=C`, and
  `$SH` set to the shell under test.
- When fixing a bug, add a case first. Every behaviour listed as done in `STATUS.md` must stay covered.
- Jobs in cases must be deterministic: either finished (after `wait`) or long-running and killed. `kill $!` on a
  background pipeline only kills its last process, so use `: | sleep 10`, not `sleep 10 | sleep 10`.
- The system dash is Debian's, which has patches (e.g. it forks the last command of `sh -c`). Upstream dash
  source is the reference for intent; `DEVIATIONS.md` records where the two matter.
- Interactive behaviour (job control, Ctrl-C/Ctrl-Z, terminal modes) is tested on a pty in `tests/interactive.rs`.
  Steps wait for output or for named processes to be in the terminal's foreground group, never for fixed times.

## Architecture

All data (arguments, variables, filenames) is bytes (`Vec<u8>`), never UTF-8 strings. Process creation uses raw
`fork`/`execve` (via `src/sys.rs`, whose wrappers retry on EINTR), not `std::process::Command`, because subshells fork
without exec. Simple external commands in non-interactive shells without job control use `posix_spawn` instead
(`can_spawn` in `exec/simple.rs`, like dash's `vforkexec`), with redirections and assignments made in the shell.

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
- **Forking**: all forks go through `exec/fork.rs::fork_child` with a `ForkKind` (a foreground or background job's
  process, with its process group, or `NoJob` for command substitution), which sets up process groups and the
  terminal under job control. Children reset traps and signal dispositions in `child_reset`, with signals blocked
  across the fork when anything is trapped or ignored. An `exit` flag (dash's `EV_EXIT`, `run_list_exit`) marks
  code after which the process exits (a forked child, the end of `-c`); its last external command then execs
  without forking (`no_fork` in `run_command`), unless a trap is set.
- **Redirections** for in-process commands save the original fds at fd ≥ 10 (close-on-exec) and restore them
  afterwards; `exec` without a command makes them permanent.
- **Expansion** (`expand/`): field splitting happens while the text is built (`split.rs`); `pattern.rs` is the
  matcher shared by globbing, `case`, and `${x#pat}`; `arith.rs` is the `$((...))` evaluator.
- **Signals** are installed without `SA_RESTART` so `wait` gets EINTR. Rust ignores SIGPIPE before `main`, so `main`
  restores the default.
- **Interactive mode** (`interactive/`) uses rustyline, kept behind its own module so the line editor stays separate
  from the executor (a Stage 3 SSH mode depends on this). The completer (`interactive/complete.rs`) and the
  syntax highlighter (`interactive/highlight.rs`) never see `Shell`: `read_line` hands them a `Names` snapshot (and
  the colours and pending text) before each prompt.
  The history store (`interactive/history.rs`) is luish's own rustyline `History`, so that `fc` gets stable event
  numbers and can replace its own entry.
- **Jobs** (`jobs.rs`) follow dash's model: numbered slots plus a "current job" order, finished jobs kept until
  reported. Without job control only background jobs are recorded; foreground commands are waited for directly.
  With job control (`Shell::jobctl` holds the terminal) every job is recorded while it runs and waited for with
  `wait_job`. Job text comes from the AST via `cmdtext.rs`.

## Conventions

- Error messages use dash's wording.
- Performance matters: hot paths (e.g. `[ ... ]` in loops) must avoid needless syscalls. Compare against dash with a
  release build when changing the executor or expansion.
- Documentation commits are prefixed `DOC`.
