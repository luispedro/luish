# luish — Implementation Plan

`luish` is a POSIX-compliant shell for Linux, written in Rust, with optional
support for plugins written in Python. The long-term aim is to replace zsh as
a daily-driver shell.

`GOALS.md` groups the goals into three stages. This plan covers Stage 1
(reproducing existing functionality) in detail, plus the plugin system from
Stage 2. §9 covers the later stages and the constraints they put on Stage 1
design.

---

## 1. Goals and non-goals

### Stage 1 goals (current focus)

- **POSIX conformance**: implement the Shell Command Language (POSIX.1-2017,
  XCU chapter 2) and the required built-ins. When the spec is ambiguous, match
  `dash`. Also implement `local` as dash does: it isn't POSIX, but dash
  supports it and many real-world `sh` scripts depend on it.
- **As fast as dash**: `luish -c true` should start in under ~2 ms, and script
  execution should match `dash`. M3 requires being within ~1.5× of dash, and
  Phase 12 closes the remaining gap. Every later feature is
  pay-for-what-you-use. For example, Python must add **no cost** unless a
  plugin is actually loaded.
- **Usable interactively as a daily driver**: line editing, history,
  completion, and job control.
- **Correct with arbitrary bytes**: arguments, variables, and filenames are
  byte strings, not UTF-8 strings.

### Deferred to later stages

These are planned, but none of them is built during Stage 1 (see §9).
Stage 1 design must not rule them out.

- **Stage 2:** Python plugins (Phase 11 and §6). Opt-in bash/zsh extensions
  such as arrays, associative arrays, process substitution, `[[ ]]`, and
  `{a,b}` brace expansion. Modern terminal features. Better scripting
  and debugging support. Richer completion and history.
- **Stage 3:** caching of login scripts, and a built-in SSH client/server mode.

### Non-goals

These are not a focus of the project. That does not mean outside
contributions for them will be rejected.

- Platforms other than Linux.

### Design constraints

- Plugins must not change parsing or expansion semantics. A POSIX script must
  behave the same whichever plugins are loaded.
- "Costs nothing when unused" is strict for scripts and `-c`: startup time,
  execution speed and memory. Interactive shells are judged on responsiveness
  and functionality instead. They may spend a reasonable amount of memory
  (a few bytes per variable, function or history entry) to support
  interactive features, as long as the prompt and line editor stay fast.

---

## 2. Key design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Core language | Rust | Fast startup, precise control over syscalls, memory safety |
| Target platform | Linux only | Linux-specific APIs (such as `pidfd`, `signalfd`, `clone(CLONE_VFORK)`, and `/proc`) may be used wherever they help. No portability layer |
| Syscalls | `nix` (or `rustix`) | Safe wrappers for fork, exec, pipes, process groups, and signals |
| Process creation | Raw `fork()` + `execve()` | Subshells need a fork *without* an exec, which `std::process::Command` can't do |
| String type | `Vec<u8>` / `OsString` throughout | POSIX data is bytes |
| Execution model | Tree-walking interpreter over an AST | Simple, and fast enough for a shell |
| Line editing | `rustyline` (or `reedline`), behind a `LineEditor` interface | Mature and supports vi and emacs modes. The interface keeps the editor separate from the executor so it can later run on an SSH client (§9.3) |
| Python | `pyo3`, embedded behind the `python` cargo feature, initialized lazily | No libpython dependency or startup cost when unused |
| Reference behaviour | `dash`, then `bash --posix` | Used to settle spec ambiguities in tests |

---

## 3. Repository layout

```
luish/
├── Cargo.toml              # [features] python = ["dep:pyo3"]
├── GOALS.md
├── PLAN.md
├── src/
│   ├── main.rs             # CLI parsing, mode selection (interactive / script / -c)
│   ├── shell.rs            # `Shell` struct: all interpreter state
│   ├── input.rs            # input sources: string, file, stdin, line editor
│   ├── lexer.rs            # tokenizer, quoting, here-doc queue, alias expansion
│   ├── parser.rs           # recursive-descent parser → AST
│   ├── ast.rs              # AST types
│   ├── expand/
│   │   ├── mod.rs          # expansion pipeline driver
│   │   ├── param.rs        # ${...} parameter expansion
│   │   ├── arith.rs        # $((...)) lexer, parser, and evaluator
│   │   ├── cmdsubst.rs     # $(...) and `...`
│   │   ├── split.rs        # IFS field splitting
│   │   ├── pattern.rs      # fnmatch-style matcher (glob, case, ${x#pat})
│   │   └── glob.rs         # pathname expansion
│   ├── exec/
│   │   ├── mod.rs          # executor: nodes → exit status / control flow
│   │   ├── simple.rs       # simple commands and command lookup
│   │   ├── pipeline.rs
│   │   ├── fork.rs         # fork helpers and child-side reset
│   │   └── redirect.rs     # redirections and fd save/restore guards
│   ├── vars.rs             # variables, exports, readonly, positional params
│   ├── options.rs          # `set` options
│   ├── jobs.rs             # job table, process groups, terminal control
│   ├── signals.rs          # signal setup, pending-signal flags, self-pipe
│   ├── trap.rs             # trap table and dispatch
│   ├── path.rs             # PATH search and command hash table
│   ├── builtins/
│   │   ├── mod.rs          # registry, special vs regular classification
│   │   └── *.rs            # one file per built-in (or small groups)
│   ├── interactive/
│   │   ├── mod.rs          # REPL loop, prompts
│   │   ├── editor.rs       # `LineEditor` interface + rustyline implementation
│   │   ├── history.rs
│   │   └── complete.rs
│   └── plugins/
│       ├── mod.rs          # plugin-agnostic traits (Builtin, Hook), registry
│       └── python.rs       # #[cfg(feature = "python")] PyO3 bridge
├── python/
│   └── luish/              # stub `.pyi` files and docs for the plugin API
├── tests/
│   ├── cases/              # *.sh test scripts plus expected output
│   ├── compare.rs          # differential harness (luish vs dash)
│   └── plugins/            # Python plugin tests
├── fuzz/                   # cargo-fuzz targets (lexer, parser, arith, pattern)
└── bench/                  # hyperfine scripts
```

---

## 4. Core data structures

### 4.1 AST

```rust
pub struct Program { pub items: Vec<CompleteCommand> }

pub struct CompleteCommand { pub list: AndOrList, pub async_: bool } // trailing `&`

pub struct AndOrList { pub first: Pipeline, pub rest: Vec<(AndOr, Pipeline)> }
pub enum AndOr { And, Or }

pub struct Pipeline { pub negated: bool, pub cmds: Vec<Command> }

pub enum Command {
    Simple(SimpleCommand),
    Compound(CompoundCommand, Vec<Redirect>),
    FunctionDef { name: Vec<u8>, body: Rc<(CompoundCommand, Vec<Redirect>)> },
}

pub struct SimpleCommand {
    pub assigns: Vec<Assign>,
    pub words: Vec<Word>,
    pub redirs: Vec<Redirect>,
    pub lineno: u32,
}

pub enum CompoundCommand {
    BraceGroup(Vec<CompleteCommand>),
    Subshell(Vec<CompleteCommand>),
    If { conds: Vec<(Vec<CompleteCommand>, Vec<CompleteCommand>)>, else_: Option<Vec<CompleteCommand>> },
    While { cond: Vec<CompleteCommand>, body: Vec<CompleteCommand>, until: bool },
    For { var: Vec<u8>, words: Option<Vec<Word>>, body: Vec<CompleteCommand> },
    Case { word: Word, arms: Vec<CaseArm> },
}

pub struct Word(pub Vec<WordPart>);

pub enum WordPart {
    Literal(Vec<u8>),                  // unquoted literal text
    SingleQuoted(Vec<u8>),
    DoubleQuoted(Vec<WordPart>),       // may contain Param / CmdSubst / Arith / Literal
    Escaped(u8),                       // backslash-quoted char
    Tilde(Option<Vec<u8>>),            // ~ or ~user (only at word start / after `:` in assignments)
    Param(ParamExp),
    CmdSubst(Rc<Program>),             // $(...) and `...` (both parsed eagerly)
    Arith(Word),                       // $((...)): expanded first, then evaluated
}

pub struct ParamExp { pub name: ParamName, pub op: ParamOp, pub colon: bool }
pub enum ParamName { Var(Vec<u8>), Positional(usize), Special(u8) } // @ * # ? - $ ! 0
pub enum ParamOp {
    Plain, Length,
    Default(Word), Assign(Word), Error(Word), Alternative(Word),
    RemoveSmallestSuffix(Word), RemoveLargestSuffix(Word),
    RemoveSmallestPrefix(Word), RemoveLargestPrefix(Word),
}

pub struct Redirect { pub fd: Option<u32>, pub kind: RedirKind, pub target: RedirTarget }
pub enum RedirKind { In, Out, Append, Clobber, ReadWrite, DupIn, DupOut, HereDoc { strip_tabs: bool } }
pub enum RedirTarget { Word(Word), HereDoc(Rc<HereDocBody>) }
pub struct HereDocBody { pub body: Word, pub quoted: bool } // quoted delimiter => no expansion
```

The AST uses `Rc` where nodes need to outlive a single execution, such as
function bodies and command substitutions.

### 4.2 Expansion intermediate form

Field splitting and globbing need to know which characters were quoted, so
expansion works on a byte string with one "quoted" flag per byte:

```rust
pub struct XChar { pub b: u8, pub quoted: bool }
pub struct XField(pub Vec<XChar>);
```

- Field splitting only splits on unquoted bytes that came *from an expansion*.
  Literal bytes are never split, so each `XChar` also needs a `from_expansion`
  flag, or equivalently a split-eligible flag.
- Globbing treats a quoted `*`, `?` or `[` as a literal.
- Quote removal is then just `field.iter().map(|c| c.b)`.

### 4.3 Shell state

```rust
pub struct Shell {
    pub vars: Vars,                    // HashMap<Vec<u8>, Var { value, exported, readonly }>
    pub positional: Vec<Vec<u8>>,      // $1..; saved/restored per function call
    pub arg0: Vec<u8>,
    pub last_status: i32,              // $?
    pub last_bg_pid: Option<Pid>,      // $!
    pub options: Options,              // -e -u -x -f -n -v -a -C -m -b -h, ignoreeof, vi/emacs
    pub functions: HashMap<Vec<u8>, Rc<FunctionBody>>,
    pub aliases: HashMap<Vec<u8>, Vec<u8>>,
    pub traps: Traps,
    pub jobs: JobTable,
    pub hash: PathHash,
    pub builtins: BuiltinRegistry,     // Rust built-ins + plugin built-ins
    pub plugins: PluginManager,
    pub interactive: bool,
    pub in_subshell: bool,
    pub loop_depth: usize,
    pub func_depth: usize,
    pub errexit_suppressed: usize,     // >0 inside if/while conditions, && / || lhs, `!`
}
```

### 4.4 Control flow

```rust
pub enum Flow { Break(usize), Continue(usize), Return(i32), Exit(i32) }
pub type ExecResult = Result<i32, Flow>;
```

Loops consume `Break` and `Continue`, function calls and `.` consume `Return`,
and the top level consumes `Exit`. A forked child that receives `Exit` or
reaches the end of its code calls `_exit`. **A child must never return into the
parent's main loop.**

---

## 5. Implementation phases

Each phase ends with a list of tests that must pass before moving on.
Phases 0–10 and 12 make up Stage 1. Phase 11 (plugins) belongs to Stage 2.

### Phase 0 — Scaffolding (½ day)

- `cargo new luish`, with `nix`, `rustyline`, and an optional `pyo3` behind the
  `python` feature.
- CI running `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, and
  `cargo test --features python`.
- **Differential test harness** (`tests/compare.rs`): for each
  `tests/cases/**/*.sh`, run the script under `luish` and `dash` and compare
  stdout, stderr (normalised, since error messages differ), and exit status.
  When a case needs its own expected output, put it in a `*.expected` file
  next to the script.

**Done when:** `cargo test` runs an empty harness.

### Phase 1 — Minimal REPL and external commands (1–2 days)

- `main.rs` supports `luish`, `luish script [args]`, `luish -c 'cmd' [arg0 args]`,
  and `luish -s`.
- Split input on whitespace (a placeholder for the real lexer).
- `PATH` search, then `fork` + `execve`, with the parent calling `waitpid`.
- Exit statuses: 127 for not found, 126 for not executable, 128+N when killed by
  signal N. If `execve` fails with `ENOEXEC`, run the file as a `luish` script
  in the child.
- Temporary built-ins: `exit` and `cd`.

**Done when:** `ls -l /`, `false; echo $?`-style tests work, once `echo` resolves
to `/bin/echo`.

### Phase 2 — Lexer (3–5 days)

- Tokens: `Word`, `IoNumber`, `Newline`, the operators
  `| || & && ; ;; ( ) < > >> <& >& <> >| << <<-`, and `Eof`.
- Quoting: `'...'`, `"..."`, and backslash, including `\<newline>` line
  continuation.
- Nested constructs: `$(...)` (**parse recursively** with a sub-parser, which
  correctly handles `case ... in x)` inside `$( )`), backquotes (unescape
  `` \` ``, `\\` and `\$`, then parse), `${...}`, and `$((...))`. Telling
  `$((` apart from `$( (` needs a fallback that re-lexes on failure.
- Reserved words are recognised by the **parser** based on position, not the
  lexer. The lexer only marks whether a word is a candidate keyword.
- **Here-documents**: when the lexer sees `<<` or `<<-`, it pushes the pending
  here-doc onto a queue. On the next newline token, it reads the bodies in
  order. Whether the delimiter was quoted decides whether the body is expanded.
- **Aliases**: substituted in command-name position. A trailing blank in an
  alias value makes the next word eligible for substitution too. Guard against
  recursive expansion.
- Track line numbers for `LINENO` and error messages.

**Done when:** golden token-stream tests pass for tricky inputs such as nested
quotes, `$(case x in x) echo;; esac)`, here-docs inside `$( )`, and multiple
here-docs on one line. The `fuzz_lexer` target runs for 10 minutes without
panics.

### Phase 3 — Parser (3–5 days)

- Recursive descent following the XCU §2.10 grammar and producing the §4.1 AST.
- Recognise assignments (`NAME=...` before the command name), reserved words in
  command position, `!`, function definitions `name() compound [redirs]`, and
  `case` patterns (`(` is optional, patterns are separated by `|`).
- **Incremental parsing**: `parse_next_complete_command()` returns one complete
  command at a time. Scripts are parsed and executed one command at a time:
  aliases defined by a command must affect the lines after it, and syntax
  errors late in a script must not stop the earlier commands from running.
- In interactive mode, an incomplete-input error triggers `PS2` and reads more
  input.
- **Reading from stdin**: when the script comes from standard input, the shell
  must leave the file offset just after the command it has read, because the
  command it runs may read stdin too. If stdin is seekable, `lseek` back after
  buffered reads. Otherwise read one byte at a time.
- `set -n`: parse without executing.

**Done when:** AST snapshot tests pass (for example with `insta`), `fuzz_parser`
runs without panics, and syntax errors report the line number.

### Phase 4 — Executor core (4–6 days)

- **Simple commands**:
  1. Expand the words.
  2. If there is no command name, apply the assignments to the shell and run
     the redirections in a subshell-like guard.
  3. Otherwise look the command up in this order: special built-in, function,
     regular built-in (including plugin built-ins), then `PATH`.
  4. Assignment scope: for special built-ins the assignments persist. For
     functions and regular built-ins they are temporary. For external commands
     they are exported only to that child.
- **Pipelines**: create N−1 pipes, fork N children, `dup2` the ends into place,
  and close all the pipe fds in both parent and children. The pipeline's status
  comes from the last command, and `!` inverts it. Every stage runs in a
  subshell, including the last one, as in dash.
- **And-or lists** and **async commands** (`&`): an async command records `$!`
  and, in a non-interactive shell, ignores SIGINT and SIGQUIT and redirects
  stdin from `/dev/null`.
- **Compound commands**: brace groups, subshells (fork, then run in the child
  with `in_subshell = true`), `if`, `while`, `until`, `for` (without `in`,
  iterate over `"$@"`), and `case`, with `case` initially using exact matching.
- **Redirections** (`redirect.rs`):
  - Apply them left to right.
  - For anything run in-process (built-ins, functions, compound commands),
    create a `RedirGuard` that first saves the affected fds with
    `fcntl(F_DUPFD_CLOEXEC, 10)` and restores them on drop.
  - Supported forms: `<`, `>` (fails under `set -C` if the file exists and is
    regular), `>|`, `>>`, `<>`, `<&n`, `>&n`, `<&-`, `>&-`, and here-docs.
    Here-doc bodies go through a pipe, or a temp file if they're larger than
    the pipe buffer.
  - Keep the shell's own fds (the script file and the saved fds) at 10 or above
    with `FD_CLOEXEC` set.
- **Fork discipline** (`fork.rs`): in the child, reset the signal dispositions
  (§5 Phase 8), clear the job table, set `in_subshell`, and flush any Rust-side
  buffered output **before** forking.

**Done when:** the pipeline, redirection, and control-structure cases in
`tests/cases/exec/` match dash.

### Phase 5 — Word expansion (5–8 days)

Implement the pipeline from §4.2 in the order POSIX requires:

1. **Tilde**: `~` expands to `$HOME` and `~user` uses `getpwnam`. In
   assignments it also applies after each unquoted `:`.
2. **Parameter**: all the `ParamOp` forms. `${x:-w}` and friends expand `w`
   lazily. `$@` inside double quotes produces separate fields, including zero
   fields when there are no positional parameters, while `"$*"` joins them with
   the first byte of IFS. `set -u` errors on unset variables, except for `$@`
   and `$*`.
3. **Command substitution**: pipe, fork the child to run the parsed `Program`,
   read everything in the parent, `waitpid`, and strip trailing newlines. Its
   exit status becomes `$?` for assignment-only commands.
4. **Arithmetic** (`arith.rs`): expand the text first, then tokenize and parse
   it with precedence climbing. The operators are the C ones: unary `+ - ! ~`,
   `* / % + - << >> < <= > >= == != & ^ | && || ?:`, `=` and the compound
   assignment operators. Values are `i64`. Variables inside the expression are
   read as integers, and unset or empty ones count as 0. Support decimal, octal
   (`0...`) and hex (`0x...`) literals. Division by zero is an error.
5. **Field splitting** (`split.rs`): IFS whitespace versus non-whitespace rules,
   empty IFS (no splitting), unset IFS (defaults to space, tab, newline), and
   empty fields from unquoted empty expansions being removed.
6. **Pathname expansion** (`glob.rs`), skipped under `set -f`: split the pattern
   into path components, run `readdir` on each level, match with
   `pattern.rs`, and sort the results (byte order at first, locale collation
   later). A leading `.` must be matched explicitly, and a pattern with no
   matches is left unchanged.
7. **Quote removal.**

`pattern.rs` is also used by `case` and by the prefix/suffix removal operators.
It supports `*`, `?`, and bracket expressions with `!` negation, ranges, and
`[:class:]`. It must respect the per-byte quoted flags.

**Done when:** the expansion test suite (which should be the largest one)
matches dash. This includes `IFS` edge cases and `"$@"` with 0, 1, and many
arguments.

### Phase 6 — Variables and built-ins (4–6 days)

`vars.rs` handles exports, readonly, and allexport (`set -a`). The environment
passed to `execve` is built from exported variables only. On startup, import
the environment and set `PPID`, `PWD` (validated against the real cwd),
`IFS`, `PS1`, `PS2`, `PS4`, and `OPTIND=1`.

Built-ins, in the order to implement them:

| Group | Built-ins |
|---|---|
| Special | `:` `.` `break` `continue` `eval` `exec` `exit` `export` `readonly` `return` `set` `shift` `times` `trap` `unset` |
| Must run in-process | `cd` (`-L`/`-P`, `CDPATH`, `OLDPWD`, `cd -`), `pwd`, `read` (`-r`, IFS splitting, backslash continuation), `umask`, `wait`, `alias`, `unalias`, `getopts`, `command` (`-v`, `-V`, `-p`), `type`, `hash`, `ulimit`, `kill` (`-l`, `-s`, job specs), `local` (Phase 7) |
| Built-in for speed | `true`, `false`, `echo` (dash-compatible: no options except `-n`, and XSI escapes), `printf` (full format support, and `%b`), `test` / `[` (following POSIX's argument-count rules) |
| Job control | `jobs`, `fg`, `bg` (Phase 10) |
| Other | `fc` (Phase 10) |

Error semantics: an error in a special built-in makes a non-interactive shell
exit. A redirection error on a special built-in does the same.

**Done when:** each built-in has its own test file in `tests/cases/builtins/`
that matches dash.

### Phase 7 — Functions, `eval`, `.`, and control flow (2–3 days)

- Function calls push a new positional-parameter frame and restore it on
  return. `return` outside a function or `.` script is an error.
- `local` (as in dash): scoping is dynamic. Each function frame records the
  previous state of every variable made local and restores it on return. A
  local variable starts with the value and the exported and readonly flags of
  the variable with the same name in the enclosing scope, or unset if there
  is none. `local -` saves the shell options and restores them on return.
  `local` outside a function is an error.
- Each frame also records the function name (or the `.` file name) and the
  line it was called from. This costs little, and it makes stack traces in
  error messages easy to add in Stage 2.
- `break N` and `continue N`: N ≥ 1 and is clamped to the current loop depth.
- `eval` concatenates its arguments with spaces, then parses and executes the
  result in the current context.
- `.` searches `PATH` when the name has no `/`, and runs the file in the
  current shell. `return` inside it ends the file.
- `exec` with no command applies its redirections permanently. With a command,
  it replaces the shell and never returns.

### Phase 8 — Signals and traps (3–4 days)

- `signals.rs`: at startup, record which signals were **ignored on entry**;
  those can never be trapped or reset. Install a handler that sets a per-signal
  `AtomicBool` and writes to a self-pipe, for waking up `poll` in interactive
  mode.
- Pending traps are checked **between commands** and whenever `waitpid`
  returns `EINTR`. A trap action runs with `$?` saved and restored.
- `trap` syntax: `trap 'action' SIG...`, `trap - SIG` (reset), `trap '' SIG`
  (ignore), `trap` alone (print traps in a re-inputtable form), and the `EXIT`
  pseudo-signal (also accepted as `0`), which runs on normal exit and on `exit`.
- **Subshells**: traps with an action are reset to the default in the child.
  Ignored signals stay ignored.
- **Interactive shell**: ignore SIGINT (the handler cancels the current line),
  SIGQUIT and SIGTERM. With job control also ignore SIGTSTP, SIGTTIN and
  SIGTTOU. Children get the defaults restored.
- Wait for a foreground child with `waitpid` in a loop.

**Done when:** trap tests pass, including `trap 'echo bye' EXIT` inside
subshells and command substitutions, and `kill -TERM $$` running a trap.

### Phase 9 — Options and `set -e` (2–3 days)

- `set -e` (errexit): exit when a command fails, **except**:
  - inside an `if`, `while` or `until` condition,
  - on the left side of `&&` or `||`,
  - anywhere after `!`,
  - in commands inside a compound command or function that was itself called
    in one of those contexts.

  Implement this by incrementing `errexit_suppressed` on the way into these
  contexts and decrementing it on the way out.
- `set -x` prints expanded commands to stderr with `PS4` expanded as the
  prefix, quoting arguments in a re-inputtable form.
- `set -u`, `-f`, `-v`, `-a`, `-C`, `-b`, `-h`, and `-o name` / `+o name`,
  including `set -o` output that can be re-read as input.
- `$-` reflects the options currently set.

### Phase 10 — Interactive mode and job control (5–8 days)

- **Startup**: when interactive, expand `$ENV` (POSIX) and source it. Also
  source `~/.config/luish/luishrc` if it exists. A shell is interactive if it
  has no command-file operand and stdin and stderr are ttys, or if `-i` is
  given.
- **Line editor** (`rustyline`): emacs mode by default and vi mode with
  `set -o vi`. A history file at `$HISTFILE`, with a size limit.
  Multi-line input: when the parser reports incomplete input, show `PS2`.
- **Editor/executor separation**: the REPL talks to the editor only through
  the `LineEditor` interface in `editor.rs`. The shell gives it a prompt and
  gets back a complete command line. The editor calls back to the shell only
  through narrow requests: "is this input complete?", "complete this word",
  and history access. Nothing in the editor touches `Shell` directly. This
  keeps it possible to run the editor in a different process on an SSH client
  later (§9.3).
- **Prompts**: expand `PS1` with parameter expansion (and, optionally,
  command substitution). Plugins can override the prompt (§6).
- **Completion**: command names from built-ins, functions, aliases and `PATH`,
  and filenames for other words. Plugins can provide completers (§6).
- **Job control** (`jobs.rs`, active under `set -m`, which is on by default in
  interactive mode):
  - On startup, loop until the shell is the foreground process group, then
    `setpgid(0, 0)` and `tcsetpgrp(tty, own_pgid)`.
  - Each pipeline becomes a job with its own process group. **Both** the parent
    and the child call `setpgid(child, pgid)` to avoid a race. The first
    process's pid is the pgid.
  - For a foreground job, call `tcsetpgrp(tty, job_pgid)`, wait with
    `WUNTRACED`, then give the terminal back to the shell. If the job stopped
    or died from a signal, restore the shell's saved terminal modes
    (`tcgetattr` / `tcsetattr`); otherwise keep the job's modes, so that
    `stty` works (as bash does).
  - Stopped jobs are recorded and reported, e.g. `[1]+  Stopped  vim`.
    `fg` continues a job with `SIGCONT` and gives it the terminal. `bg`
    continues it in the background.
  - Job specs: `%n`, `%%`, `%+`, `%-`, `%string`, and `%?string`.
  - Before each prompt, reap finished background jobs with
    `waitpid(-1, WNOHANG)` and report them. (`set -b`, reporting them
    immediately, is deferred: it would have to interrupt the line editor.)
  - The `jobs` format, the job text and the job table's lifetime rules follow
    dash exactly (dash's `showjob`, `cmdtxt`, `makejob`/`freejob`).
  - On exit, warn once if there are stopped jobs.
- `fc`: `-l` lists history, `-e editor` edits and re-runs, `-s` substitutes
  and re-runs.

**Done when:** manual checks pass (`vim`, then Ctrl-Z, `fg`, `sleep 100 &`,
`jobs`, `kill %1`, and Ctrl-C at the prompt and during a pipeline), and pty
tests pass (`tests/interactive.rs`: a small pty harness on `libc` rather than
`expectrl` or `rexpect`).

### Phase 11 — Plugin system (Stage 2, 5–7 days)

See §6 for the design. This phase starts only once Stage 1 is usable (M4).

1. Add a plugin-agnostic `plugins/mod.rs` with the `Builtin` and `Hook` traits.
   Move the Rust built-ins onto the `Builtin` trait so that plugins and native
   built-ins go through the same code path.
2. Add the `plugin` built-in: `plugin load <path|module>`, `plugin list`, and
   `plugin unload <name>`.
3. Add the PyO3 bridge (`plugins/python.rs`) behind `#[cfg(feature = "python")]`.
4. Write the Python API stubs (`python/luish/__init__.pyi`) and documentation.
5. Add plugin tests: `cargo test --features python` runs `tests/plugins/*.py`
   through the harness.

### Phase 12 — Conformance and performance hardening (ongoing)

- Port relevant cases from existing shell test suites. The Oils project's spec
  tests are written to run against multiple shells, and the smoosh and
  modernish test suites are also useful.
- **Real-world test**: run autoconf-generated `configure` scripts and large
  scripts from distributions under `luish` and compare the results with dash.
- **Benchmarks** (`bench/`, with `hyperfine`):
  - `luish -c true` compared with `dash -c true`.
  - A loop of 1M arithmetic increments.
  - String-heavy parameter expansion.
  - A fork-heavy pipeline loop.
- **Optimisations**, applied only where profiling shows a need:
  - Cache `PATH` lookups (`hash`).
  - Avoid allocations in the expansion of plain literal words.
  - Use `vfork` / `clone(CLONE_VFORK)` / `posix_spawn` for simple external
    commands that have no in-child work.
  - Intern variable names.
- Keep fuzzing the lexer, parser, arithmetic, and pattern matcher.

---

## 6. Plugin system design

### 6.1 Principles

- **Opt-in and lazy.** Plugins are loaded only by explicit `plugin load`
  commands, usually from `luishrc`. The Python interpreter starts on the first
  `plugin load` of a Python plugin. `luish` built without the `python` feature
  prints a clear error for Python plugins.
- **No semantic changes to POSIX.** Plugins can add built-ins and hooks but
  cannot change the parser or expansion. `luish --no-plugins` and scripts run
  with `luish script.sh` load nothing unless the script loads plugins itself.
- **Plugins can't crash the shell.** Python exceptions are caught, a traceback
  is printed to stderr, and the command gets status 1. A failing hook is
  reported and then skipped.

### 6.2 Rust-side traits

```rust
pub trait Builtin {
    fn name(&self) -> &[u8];
    fn special(&self) -> bool { false }   // plugins can never register special built-ins
    fn run(&self, sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult;
}

pub enum HookKind { Precmd, Preexec, Prompt, Chpwd, Complete, Exit }

pub trait Hook {
    fn kind(&self) -> HookKind;
    fn call(&self, sh: &mut Shell, ev: &HookEvent) -> HookOutcome;
}
```

Plugin built-ins rank as **regular** built-ins in command lookup, so a shell
function with the same name overrides them. A plugin can't replace a special
built-in.

### 6.3 Python API (`import luish`)

The `luish` module is registered as a built-in module with PyO3 before the
interpreter is initialized.

```python
import luish

@luish.builtin("greet")
def greet(ctx: luish.Context, argv: list[str]) -> int:
    name = argv[1] if len(argv) > 1 else (ctx.getvar("USER") or "world")
    ctx.stdout.write(f"hello {name}\n")
    return 0

@luish.hook("prompt")
def prompt(ctx) -> str:
    return f"{ctx.getvar('PWD')} $ "

@luish.hook("preexec")
def log(ctx, cmdline: str) -> None: ...

@luish.hook("precmd")
def before_prompt(ctx, last_status: int) -> None: ...

@luish.hook("chpwd")
def on_cd(ctx, old: str, new: str) -> None: ...

@luish.completer("git")
def complete_git(ctx, words: list[str], index: int) -> list[str]: ...
```

`Context` API:

| Member | Description |
|---|---|
| `getvar(name) -> str \| None`, `setvar(name, value, export=False)`, `unsetvar(name)` | Shell variables. Setting a readonly variable raises `luish.Error` |
| `getvar_bytes`, `setvar_bytes` | Byte-exact variants |
| `argv0`, `positional` | `$0` and `$1...` |
| `last_status` | `$?` |
| `stdin`, `stdout`, `stderr` | Unbuffered file objects on the **current** fds 0, 1 and 2, so redirections like `greet > f` apply |
| `run(script: str) -> int` | Parse and execute shell code in the current shell. Re-entrant |
| `capture(script: str) -> tuple[int, str]` | Like `$(...)`: runs in a subshell and captures stdout |
| `cwd`, `chdir(path)` | Changes the directory as `cd` would, updating `PWD` and running `chpwd` hooks |
| `interactive` | bool |

Arguments are passed to Python as `str`, decoded with `surrogateescape` so that
the original bytes survive a round trip. The `*_bytes` variants are there for
exact control.

### 6.4 Embedding constraints (`plugins/python.rs`)

1. **Signals**: initialize the interpreter with `Py_InitializeEx(0)` (the
   PyO3 embedding default) so that Python **doesn't install signal
   handlers**. `luish` keeps full control of SIGINT and the job-control
   signals. KeyboardInterrupt support: when SIGINT arrives while a Python
   built-in is running, call `PyErr_SetInterrupt`.
2. **Fork**: after `fork()`, if the child may run Python (for example a
   Python built-in as a pipeline stage, or inside a subshell or command
   substitution), call `PyOS_AfterFork_Child()` in the child. Call
   `PyOS_BeforeFork` / `PyOS_AfterFork_Parent` around the fork in the parent
   when Python is initialized.
3. **Threads**: plugins must not start threads, because forking a
   multithreaded process is unsafe. Document this, and emit a warning at fork
   time if `threading.active_count() > 1`.
4. **Buffered output**: flush `sys.stdout` and `sys.stderr` before every fork
   and after every Python built-in or hook returns.
5. **GIL**: `luish` is single-threaded. Acquire the GIL for each call into
   Python and release it on return.
6. **Linking**: the `python` feature links against libpython for one Python
   version. Release builds come in two variants: `luish`, with no Python, and
   `luish-py`, built with the `python` feature. Loading libpython dynamically
   at runtime is a possible later improvement.
7. **Module search**: `plugin load foo` imports `foo` from `sys.path`, with
   `~/.config/luish/plugins` prepended. `plugin load ./foo.py` loads a file
   directly.

---

## 7. Testing strategy

| Layer | Tooling | What it covers |
|---|---|---|
| Unit tests | `cargo test` | Lexer tokens, parser ASTs (`insta` snapshots), arithmetic, pattern matching, IFS splitting |
| Differential tests | `tests/compare.rs` | Script output and status compared with `dash` (and `bash --posix` where useful) |
| Expected-output tests | `*.sh` + `*.expected` | Cases where dash's behaviour is wrong or luish deliberately differs |
| Conformance | Ported suites (Phase 12) | Coverage of the spec |
| Interactive | pty harness in `tests/interactive.rs` | Prompts, line editing, job control, Ctrl-C and Ctrl-Z |
| Fuzzing | `cargo-fuzz` | No panics in the lexer, parser, arithmetic, or pattern matcher; a round-trip property that pretty-printing then re-parsing an AST gives the same AST |
| Plugins | `cargo test --features python` | API behaviour, redirection of plugin output, fork safety, exception handling |
| Performance | `hyperfine` in CI (non-blocking) | Catch startup and loop regressions |

Every bug fix comes with a test case in `tests/cases/`.

---

## 8. Milestones

| Milestone | Stage | Phases | Definition of done |
|---|---|---|---|
| **M1: Runs simple scripts** | 1 | 0–4 | Pipelines, redirections, `if`/`for`/`while`/`case`, and external commands work |
| **M2: POSIX script engine** | 1 | 5–9 | All expansions, built-ins, functions, traps and `set -e`. Passes the differential suite |
| **M3: Real-world scripts** | 1 | 12 (partly) | Runs autoconf `configure` scripts correctly. Performance is within ~1.5× of dash |
| **M4: Daily-driver interactive shell** | 1 | 10 | Line editing, history, completion and job control, behind the `LineEditor` interface |
| **M5: Python plugins** | 2 | 11 | Plugin built-ins and hooks work. Example plugins ship: a git-aware prompt, a `json` query built-in, and a command-timing preexec/precmd pair |

Stage 1 is complete at M4, plus performance that matches dash (Phase 12).
Milestones for the rest of Stage 2 and for Stage 3 will be planned once
Stage 1 is done.

A rough total for M1–M5 is 8–12 weeks of focused work. The largest and least
predictable parts are expansion (Phase 5), interactive mode and job control
(Phase 10), and the long tail of conformance work in Phase 12.

---

## 9. Later stages

This section is a sketch, not a plan. It records what the later stages need
from Stage 1, so that Stage 1 doesn't make them harder.

### 9.1 Stage 2: beyond POSIX

- **Extensions** (arrays, associative arrays, process substitution, `[[ ]]`,
  and brace expansion) are enabled with an option such as
  `set -o luish-extensions`. When the option is off, POSIX scripts must parse
  and behave exactly as before and run just as fast. The lexer and parser
  should keep one clear place to check the option, rather than scattering
  checks through the code.
- **Terminal features**: semantic prompt markers (OSC 133) and working
  directory reporting (OSC 7) are emitted from the REPL around the prompt and
  command output. Unicode width handling and bracketed paste belong to the
  line editor.
- **Scripting**: error messages with file, line and function stack (using the
  call frames from Phase 7), `pipefail`, a predictable strict mode, and a
  debugger or step-trace mode.
- **Variable provenance**: a built-in (name to be decided, e.g. `whereset`)
  that shows where each variable was set, like a more informative `env`:

  ```
  PATH    ~/.profile:12            prepended /home/lp/bin
          ~/.zshrc:88 → conda activate → conda.sh:412   prepended /opt/conda/bin
          inherited (sshd-session, pid 1234)
  EDITOR  ~/.profile:3
  LANG    inherited (systemd --user)
  ```

  - Each `Var` records an origin: inherited from the environment, set by the
    shell itself (`PWD`, `PPID`, `IFS` defaults), set by a built-in (`cd`,
    `read`, `getopts`), or set at a file and line (file names interned, so
    the origin is a few bytes). For assignments inside a function, the origin
    also records the call stack (from the Phase 7 frames), since
    `conda.sh:412` alone doesn't say who called `conda activate`. Text run by
    `eval` is recorded as "eval at FILE:LINE".
  - Interactive shells record the origin always, and keep the full history of
    changes for each variable, not just the last one. For list-like variables
    (`PATH`, `MANPATH`, ...), each step shows which components it added or
    removed. Scripts and `-c` record nothing unless an option such as
    `set -o trackvars` is set, and the check must not slow down assignment
    in loops.
  - Inherited variables can only be traced heuristically. Walk up the process
    tree (`/proc/PID/stat`) and report the oldest ancestor whose
    `/proc/PID/environ` has the same value. This has limits:
    `/proc/PID/environ` is the environment at exec time, so it gives the
    process that introduced a variable, not the line. Exited ancestors break
    the chain. Other users' processes (e.g. root's `sshd`) can't be read, so
    PAM, `/etc/environment` and `systemd --user` can't be told apart. A luish
    started by another luish could receive exact origins from its parent
    through an opt-in environment variable.
  - The Stage 1 requirement is that all assignments go through one function
    (`Vars::set` or `Shell::set_var`), and that the shell tracks the current
    file name as well as `lineno`: for `.`, the main script, `$ENV` and the
    rc files. File, line and stack error messages need the same thing.
- **Interactive**: richer completion, and history shared across sessions with
  metadata (working directory, exit status, duration). `history.rs` should own
  the storage format so that it can move from a plain `$HISTFILE` to a
  structured store without changing the editor.

### 9.2 Stage 3: caching of login scripts

The goal is to cache the *effects* of login scripts, not just their parse.
With a warm cache, a new shell should start almost instantly.

1. **Parse cache (first step).** Store the parsed AST of sourced files, keyed
   by path, size, mtime (or a content hash), and the alias table at the
   point the file was sourced. Aliases are expanded while lexing, so the same
   file can parse differently depending on which aliases exist. This needs the
   AST to be serializable, including the `Rc` nodes.
2. **Effect cache (final goal).** Snapshot the shell state that login scripts
   produce: variables and exports, functions, aliases, options, and possibly
   traps. On a cache hit, restore that snapshot instead of running the
   scripts. This changes semantics, so it must be explicit. Open questions:
   - What invalidates the snapshot? Which files were sourced is easy to track,
     and variable provenance (§9.1) records which file set each variable.
     The environment the scripts read, and the output of commands they ran
     (for example `$(brew --prefix)`), are not.
   - How is caching opted into: all at once, or per block with declared
     dependencies (for example a `cache` built-in wrapping part of a script)?
   - What about side effects that can't be cached, such as starting an agent
     or writing files?

The Stage 1 constraint is that shell state lives in `Shell` rather than in
globals, so that it can be snapshotted and restored.

### 9.3 Stage 3: SSH client/server mode

The line editor runs on the local client, so typing is instant, while commands
run on the remote host.

- **Transport**: standard SSH. `luish` on the client runs something like
  `ssh host luish --serve` and speaks a protocol over its stdin and stdout.
  The only requirement is that `luish` can be started on the remote host. No
  daemon and no extra network ports are needed, unlike mosh, which needs UDP
  ports opened.
- **Line editing mode**: the client shows the prompt the server sends, edits
  locally, and sends complete command lines. Completion and "is this input
  complete?" are round trips to the server, which is why §5 Phase 10 keeps
  them as narrow requests. History can live on the client and be shared
  across hosts.
- **Pass-through mode**: while a command runs, the server runs it on a pty
  that it allocates itself. The client forwards terminal input and output
  as-is, plus window-size changes. Full-screen programs such as (neo)vim
  therefore work as they do over SSH today, with the remote program driving
  the terminal. When the command finishes, the server switches back to line
  editing mode.
- **Open questions**: installing or uploading the server binary when the
  remote host doesn't have one; protocol versioning between client and server
  builds; and what happens when the connection drops. mosh survives dropped
  connections, but plain SSH does not.

---

## 10. Risks and mitigations

| Risk | Mitigation |
|---|---|
| POSIX ambiguities and differences between shells | Treat dash as the reference. Record deliberate differences in `tests/cases/**/*.expected` and in `DEVIATIONS.md` |
| Getting `set -e` wrong | Implement it with a single suppression counter (§5 Phase 9), backed by a dedicated test file |
| Terminal and process-group races in job control | Call `setpgid` in both parent and child. Block signals across `fork` until the child has reset its dispositions. Do all terminal handover through `jobs.rs` |
| Python and `fork` interacting badly | The rules in §6.4, plus a warning when plugins have started threads |
| Non-UTF-8 data | Use `Vec<u8>` everywhere in the core. Convert only at the Python boundary, using `surrogateescape` |
| Scope creep into later stages | Build no Stage 2 or 3 features until Stage 1 is usable (M4). Keep extensions behind a `set -o luish-extensions` (or similar) option |
| Stage 1 design ruling out later stages | Follow the constraints in §9: keep the `LineEditor` interface narrow, keep all state in `Shell`, and record call frames |
| Python adding startup cost | Load Python lazily, keep it behind a feature flag, and benchmark `-c true` in CI |

---

## 11. References

- POSIX.1-2017, XCU chapter 2 "Shell Command Language": §2.2 (quoting),
  §2.3 (token recognition), §2.6 (expansions), §2.7 (redirection),
  §2.9 (commands), §2.10 (grammar), §2.14 (special built-ins).
- The POSIX pages for `sh`, `set`, `trap`, `read`, `test`, `printf`, and `getopts`.
- The source code of `dash`, a compact and close-to-POSIX reference.
- mrsh (a minimal POSIX shell in C) and the Oils project's blog posts on shell
  parsing.
- PyO3's guide to embedding Python in Rust.
- The glibc manual's chapter "Implementing a Job Control Shell".
