# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

luish is a POSIX `sh` for Linux, written in Rust, meant eventually to replace zsh as a daily-driver shell. dash is the
reference implementation: where POSIX is ambiguous, match dash. It must be as fast as dash, and anything beyond POSIX
must cost nothing when unused. That is strict for scripts and `-c` (time and memory). Interactive shells are judged on
responsiveness and functionality instead: a few bytes per variable or history entry for interactive features is fine,
as long as the prompt and line editor stay fast. A change in behaviour must be opt-in; a pure extension (giving a
meaning to what is an error in dash, such as `a=(x y)`) may be always on.

## Project documents

- `GOALS.md`: the goals, grouped into stages. When goals conflict, the earlier stage wins. Stage 1 (POSIX, dash speed,
  interactive daily driver) is the current focus.
- `PLAN.md`: what is still to be built, by phase (the phase numbers are used throughout the other documents),
  starting with the next steps in priority order.
- `DEVELOPING.md`: developer documentation: design decisions, implementation notes by area with the tests that cover
  them, deviations and their tests, dash's quirks, conformance results and performance notes (the timings themselves
  are in `docs/performance.md`). **Update it (and the user docs) in the same commit as any change in behaviour.**
- `docs/compatibility.md`: deliberate differences from dash, grouped by what luish follows instead (zsh is preferred
  beyond POSIX), and the known limitations. Each deviation needs a test (a `# reference: zsh` case, or else a
  `.expected` one), listed in `DEVELOPING.md`.
- `ChangeLog`: user-visible changes per release, newest first, with pending ones under `Unreleased` at the top.
  Add a very short line (a few words, one per change) in the same commit as any improvement, new feature or bug fix
  that users would notice; skip internal refactors, tests and documentation-only changes. On release, rename
  `Unreleased` to `Version X.Y.Z YYYY-MM-DD by luispedro`.
- `working-memory.md` (untracked): a scratch pad for handing work over between sessions. Anything durable belongs in
  one of the documents above.
- `docs/`: user-facing documentation (Sphinx with MyST Markdown), published on Read the Docs via
  `.readthedocs.yaml`. Its Python dependencies are in both `docs/requirements.txt` (for Read the Docs) and the
  `docs` feature in `pixi.toml`; keep them in step.
- `docs/builtins/`: one Markdown page per built-in, which `help` shows (compiled in, see `src/builtins/help.rs`)
  and `docs/builtins.md` includes. Keep them up to date with the built-ins' behaviour.

## Commands

pixi coordinates everything. Run cargo through the pixi tasks or as `pixi run cargo ...`. dash and bash come from the
system (`/usr/bin`), not pixi: the conda-forge `dash` package is Plotly Dash, not the shell. zsh comes from pixi.

```sh
pixi run check                    # cargo fmt --check, clippy --all-targets -D warnings, cargo test (must stay green)
pixi run test                     # tests only
LUISH_CASE=expand/ pixi run test  # only differential cases whose path contains the substring
pixi run cargo test --bin luish parser::   # unit tests in one module
pixi run luish -c 'echo hi'       # run the shell
pixi run release                  # release build (LTO, panic=abort), used for benchmarks
pixi run bench                    # release build, then the script benchmarks in bench/ (see bench/README.md)
pixi run dist                     # release packages in target/dist/ (DEVELOPING.md, Releases); needs rustup for musl
pixi run docs                     # build the user docs into docs/_build/html (warnings are errors)
```

`rustfmt.toml` sets `max_width = 120`. Plugins (Phase 11, `src/plugins/`) use Rhai behind the default `plugins` cargo
feature; `cargo clippy --all-targets --no-default-features` must also stay clean. A **plugin** is what
`plugin load` loads (a directory of Rhai and shell files, a `.rhai` file or a `.lsh` file); its **extension** is its
Rhai code (the directory's `extension.rhai`, or the `.rhai` file), which runs in the shell. Keep the two words apart, as
zsh users read "plugin" as files to source.

## Tests

Most coverage is differential (`tests/compare.rs`): each `tests/cases/**/*.sh` runs under luish and under dash.

- stdout and the exit status must match exactly. For stderr only emptiness is compared, unless the script contains
  the line `# stderr: exact`.
- A script with the line `# reference: zsh` is compared with `zsh --emulate sh` instead of dash, for a deliberate
  deviation where luish follows zsh. Words after `zsh` are further arguments (e.g. `# reference: zsh -o noposixcd`).
  zsh comes from pixi (pinned in `pixi.toml`), so run such cases through pixi.
- `NAME.expected` (with optional `NAME.status`) replaces the reference run, for deviations that no reference shell
  matches. Record every deviation in `docs/compatibility.md`, and its tests in `DEVELOPING.md`. `NAME.stdin` is fed
  to standard input.
- Each script runs in a fresh temporary directory that is also `$HOME`, with a cleared environment, `LC_ALL=C`, and
  `$SH` set to the shell under test, in a session of its own (so without a controlling terminal).
- When fixing a bug, add a case first. Every behaviour listed in `DEVELOPING.md` must stay covered.
- Jobs in cases must be deterministic: either finished (after `wait`) or long-running and killed. `kill $!` on a
  background pipeline only kills its last process, so use `: | sleep 10`, not `sleep 10 | sleep 10`.
- The system dash is Debian's, which has patches (e.g. it forks the last command of `sh -c`). Upstream dash
  source is the reference for intent; `DEVELOPING.md` records where the two matter.
- Plugin cases (`tests/plugins/*.sh`) can't run under dash: each needs `NAME.expected`, and stderr must be empty or
  match `NAME.stderr`. They get `$STD_PLUGINS` (the path of `luish-std-plugins`), and test completers with
  `__luish_internal complete LINE`, which prints what Tab offers.
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
  process, with its process group, or `NoJob` for command and process substitution), which sets up process groups
  and the terminal under job control. Children reset traps and signal dispositions in `child_reset`, with signals
  blocked across the fork when anything is trapped or ignored. An `exit` flag (dash's `EV_EXIT`, `run_list_exit`) marks
  code after which the process exits (a forked child, the end of `-c`); its last external command then execs
  without forking (`no_fork` in `run_command`), unless a trap is set.
- **Redirections** for in-process commands save the original fds at fd ≥ 10 (close-on-exec) and restore them
  afterwards; `exec` without a command makes them permanent.
- **Expansion** (`expand/`): field splitting happens while the text is built (`split.rs`); `pattern.rs` is the
  matcher shared by globbing, `case`, and `${x#pat}`; `arith.rs` is the `$((...))` evaluator.
- **Signals** are installed without `SA_RESTART` so `wait` gets EINTR. `main` is a C `main` (`#![no_main]`),
  so Rust's start-up doesn't run: SIGPIPE is left as inherited, closed fds 0 to 2 stay closed, and there is no stack
  overflow handler (`stack.rs` makes deep nesting an error instead). Whether a signal was ignored on entry is looked up lazily (`signals::ignored_on_entry`).
- **Interactive mode** (`interactive/`) uses rustyline, kept behind its own module so the line editor stays separate
  from the executor (a Stage 3 SSH mode depends on this). The completer (`interactive/complete.rs`) and the
  syntax highlighter (`interactive/highlight.rs`) never see `Shell`: `read_line` hands them a `Names` snapshot (and
  the colours and pending text) before each prompt.
  The history store (`interactive/history.rs`) is luish's own rustyline `History`, so that `fc` gets stable event
  numbers and can replace its own entry. History expansion (`interactive/bang.rs`) rewrites each line the editor
  returns, before it is parsed.
- **Jobs** (`jobs.rs`) follow dash's model: numbered slots plus a "current job" order, finished jobs kept until
  reported. Without job control only background jobs are recorded; foreground commands are waited for directly.
  With job control (`Shell::jobctl` holds the terminal) every job is recorded while it runs and waited for with
  `wait_job`. Job text comes from the AST via `cmdtext.rs`.

## Conventions

- Error messages use dash's wording.
- Performance matters: hot paths (e.g. `[ ... ]` in loops) must avoid needless syscalls. Compare against dash with a
  release build when changing the executor or expansion (`bench/run.sh`).
- Documentation commits are prefixed `DOC`.
- Other sessions may work on this repository at the same time (in worktrees, or committing to `main`). Stage only
  explicit paths (`git add src tests DEVELOPING.md ...`), never `git add -A` at the root.
