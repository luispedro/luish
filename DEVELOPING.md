# Developing luish

This is the developer (and agent) documentation: how luish is built and tested, why it is built that way, what is
implemented and which tests cover it, and what is known about dash that the code depends on. User-facing behaviour is
documented in `docs/` (with the differences from dash and the known limitations in `docs/compatibility.md`); what is
still to be built is in `PLAN.md`, and the goals are in `GOALS.md`. `CLAUDE.md` has the commands, the test
conventions and an overview of the architecture; this file doesn't repeat them.

**Keep this file current**: update it (and the user docs) in the same commit as any change in behaviour. Every
behaviour listed here must stay covered by the tests named with it.

## Design decisions

| Decision | Choice | Rationale |
|---|---|---|
| Target platform | Linux only | Linux-specific APIs (`/proc`, `clone(CLONE_VFORK)` through `posix_spawn`, ...) may be used wherever they help. No portability layer |
| Process creation | Raw `fork` + `execve`, and `posix_spawn` where possible | Subshells need a fork *without* an exec, which `std::process::Command` can't do |
| String type | `Vec<u8>` throughout | POSIX data is bytes |
| Execution model | Tree-walking interpreter over an AST | Simple, and fast enough for a shell |
| Line editing | rustyline, kept inside `interactive/` | Mature, with vi and emacs modes. The completer and highlighter get a plain-data snapshot (`Names`) instead of `Shell`, which keeps the editor separable for the SSH mode of Stage 3 |
| Plugins | Rhai, behind the `plugins` cargo feature, with the engine created on the first extension loaded | Pure Rust, so no system dependency. No threads, global state or signal handlers, so it is safe across `fork`. Scripts can be interrupted and resource-limited |
| Reference behaviour | dash (upstream for intent), then zsh beyond POSIX | See `docs/compatibility.md` |

**Why Rhai.** The first plan used Python (through PyO3), dropped because linking libpython makes the dynamic loader
map it at every startup and makes the login shell depend on the system Python; an initialized interpreter
complicates forking without exec (the GIL, at-fork hooks, threads started by extensions) and signal handling; and a
runaway or crashing extension can hang or kill the shell. Rhai's costs are strings that can only hold UTF-8 (see
[Plugins](#plugins)), no library ecosystem (the `sh`, `fs` and `vcs` modules provide what extensions need), and a
language few people know. Lua (through `mlua`) was the main alternative: 8-bit clean strings and faster, but it
brings C code into the build.

**Why `toml-span`** for `config.toml`: it is small (no serde; its only dependency is `smallvec`) and keeps the
positions of keys and values, which give the lines for errors and the order of the keys (its tables are sorted maps,
so `config.rs` sorts entries by position; the order matters for `plugins.enabled`, which loads in the file's
order). TOML's bare keys can't contain `/`, so `plugins.enabled` takes `SOURCE.NAME = "*"` (a table, told apart from
an inline source by having none of `gh`, `git` and `path`) as well as the quoted `"SOURCE/NAME" = "*"`.

**Why git plugins are fetched by running `git`**, not with libgit2 or gitoxide: no dependency, and git's own
configuration applies (credentials, SSH keys, proxies, `insteadOf`). Only `plugin sync` and `plugin update` run it;
startup reads `plugins.lock` and looks for the extracted commits, so it never touches the network.

**Grouped settings.** luish's own options are bits in `Options` (so nothing on a hot path changes), named
`group.name` in `EXTENDED` (`options.rs`), with their earlier names and zsh's in `ALIASES`. Settings with a value
(`VALUES`) are backed by the variables they have always been (`history.file` is `$HISTFILE`), since `HISTFILE` and
`HISTSIZE` are POSIX variables and zsh shares the file. Names are only looked up when `setopt` runs. `-p` rather than
`-g` for the group, because `-g` means "global" to zsh's `typeset`.

## Repository layout

```
src/
├── main.rs             # C `main` (#![no_main]): command line, mode selection
├── shell.rs            # `Shell`: all interpreter state
├── sys.rs              # syscall wrappers (retry on EINTR)
├── input.rs            # input sources: string, file, stdin, line editor
├── lexer.rs, parser.rs # one `Parser` struct: tokens, quoting, here-docs, aliases, recursive descent
├── ast.rs              # AST types
├── cmdtext.rs          # job text from the AST (dash's cmdtxt)
├── unparse.rs          # AST back to source text that re-parses exactly
├── prompt.rs           # zsh's % sequences (setopt prompt.percent)
├── state.rs            # the shell's state as commands (savestate), and differences of states
├── startcache.rs       # cached rc.d / login.d
├── config.rs           # config.toml
├── expand/             # mod.rs (driver), param.rs, arith.rs, cmdsubst.rs, split.rs, pattern.rs, glob.rs, qual.rs
├── exec/               # mod.rs, simple.rs (commands, lookup), pipeline.rs, fork.rs, redirect.rs
├── vars.rs, options.rs, jobs.rs, signals.rs, trap.rs, path.rs
├── builtins/           # mod.rs (table, special vs regular), one file per built-in or small group; help.rs
├── interactive/        # mod.rs (REPL, rustyline helper), history.rs, histfile.rs, complete.rs, menu.rs,
│                       # keys.rs, highlight.rs
└── plugins/            # mod.rs (the `plugin` built-in), package.rs (config.toml's [plugins], plugin.toml,
                        # plugins.lock), fetch.rs (git), rhai.rs, fs.rs, vcs.rs, bytes.rs
tests/
├── cases/              # differential cases (*.sh, with .expected/.status/.stdin where needed)
├── plugins/            # plugin cases (*.sh with .expected, .stderr)
├── compare.rs          # the differential harness
└── interactive.rs      # pty tests
bench/                  # script benchmarks (see bench/README.md)
docs/                   # user documentation; docs/builtins/ is compiled into `help`; docs/examples/ has example plugins
luish-std-plugins/      # a collection of plugins (git-completion, bash-completion), see its README.md
```

## Implementation notes by area

### Command line (`main.rs`)

- `-i` forces interactive mode; with `-c` or a script it runs that (it reads stdin only without them). As in dash,
  `+c` works like `-c`, `-l` (or `+l`) makes a login shell, and `-o interactive` and `-o stdin` work like `-i` and
  `-s`. `-o NAME` takes any option named as for `setopt`. Long options are listed in `docs/usage.md`. Tests:
  `options/interactive_c.sh`, `options/command_line.sh`, `tests/plugins/no-plugins.sh`.
- A missing script prints `cannot open X: No such file` and exits with 127. A script without `#!` (ENOEXEC) is
  re-run with this executable.

### Parsing (`lexer.rs`, `parser.rs`)

- Line continuation works inside `$` expansions (`$\<newline>?`), as in dash. Backquotes are unescaped, then parsed
  separately. `$((` falls back to `$( (...) )` (a deviation). Tests: `parse/*`, `parse/arith_fallback.sh`.
- Here-docs: several on one line, inside `$(...)`; bodies over 64 KiB go through an unlinked temporary file.
- Aliases: a value ending in a blank makes the next word eligible wherever it is (also a `for` variable, `in`, or a
  `case` word, as in dash). Test: `parse/alias_blank_compound.sh`.
- `AliasMap` holds regular and global aliases in one table (a name is one or the other, as in zsh) and suffix aliases
  in another. Regular and suffix aliases are expanded by the parser in command position (`maybe_expand_alias`);
  global ones by `peek` for every token, only while `AliasMap::has_globals` (a count), except for a here-document
  delimiter (`next_raw`). All three splice text into `src` (`splice_alias`). A suffix alias being expanded is
  recorded in `active_aliases` as NUL plus its suffix, so it can't clash with a name. New parsers share one empty
  table (`NO_ALIASES`), so an `eval` doesn't allocate one. The completer's `Scan` mirrors these rules. Tests:
  `parse/alias_global.sh` (zsh), `parse/alias_suffix.sh` (zsh), unit tests in `complete.rs`.
- Function bodies may be any command (`f() echo hi`), as in dash.
- As in dash, a bad `${...}` (such as `${x//a/b}`) is an error only when expanded, and `$(` in a here-doc delimiter
  is a syntax error. Test: `parse/dash_lenient.sh`.
- `Parser::started` tells a buffer of blank lines apart from a real incomplete command. The lexer reads a trailing
  `(...)` as `WordPart::GlobQual` only under `glob.bare_qualifiers`: the only place it depends on an option.
- Unit tests in `parser.rs`. No `insta` snapshots or fuzz target yet.

### Expansion (`expand/`)

- A double-quoted part is always a field, even when empty (`"$u"`, `"${u+x}"`), except a lone `"$@"` with no
  parameters. In command words `$@`, `$*` and `"$@"` give separate fields even when IFS is empty; elsewhere they are
  joined with the first character of IFS, as in dash.
- `$@` and `$*` always count as set for `${@-x}`/`${@+x}`, and are null for `${@:-x}` when their joined length is
  zero (counting separators by dash's rules, `varvalue`); `${#@}` is the joined length. Test:
  `expand/positional_ifs.sh`.
- Arithmetic: a variable holding only blanks is 0. Quotes and backslashes inside `$((...))` are kept, so they are
  errors, as in dash. Test: `expand/arith_quotes.sh`.
- Command substitution drops NUL bytes (so does `read`) and sets `$?` only for commands of assignments alone (so
  `echo $(exit 3)$?` prints 0). Test: `expand/nul_bytes.sh`.
- Globbing: byte order, `.*` matches `.` and `..` (as in dash), `^` is not negation. A lone `[` without a matching
  `]` is not a pattern: treating it as one made every `[ ... ]` call `readdir` and loops 7× slower.
- `glob.star` (`glob.rs`): no hidden directories and no links (`***/` follows them, stopping at a link to a directory
  it is already in; zsh loops). Types come from `d_type` where available. Tests: `expand/globstar.sh` (zsh),
  `expand/globstar_off.sh`, `expand/globstar_loop.sh`.
- Glob qualifiers (`qual.rs`): kept as text and recognized when a field ends in an unquoted `(...)` at glob time, so
  that, as in zsh's `sh` emulation, they can come from an expansion. Supported: file type, permission, owner,
  device, link count, size and time tests, `^ - , N D n`, `o`/`O` (`n L l a m c d N`), subscripts (from 0), `M`,
  `T`, and `:h :t :r :e :u :l`. Errors have status 1. `savestate` sets the option before the functions, and wraps a function
  with a qualifier in `set -o`/`+o` when it is off. Tests: `expand/glob_qualifiers.sh`,
  `expand/glob_qualifier_errors.sh`, `builtins/internal_savestate_globqual.sh`, unit tests in `qual.rs` and
  `parser.rs`.
- Unit tests in `split.rs`, `pattern.rs` and `arith.rs`; cases in `expand/*`.

### Execution (`exec/`)

- Simple commands follow dash's `evalcommand` order: expand words (one at a time until the command is known, for
  declaration built-ins), make the redirections in the shell (for every command kind; a forked external command
  inherits them), then expand the assignments, so `x=$(cat) <<EOF` reads the here-doc. The `set -x` trace goes to the
  stderr from before the redirections. `RedirError::Open` (status 2; exits for a special built-in) vs
  `RedirError::Flow` (fatal expansion error). Test: `exec/assign_redirect_order.sh`.
- Assignments before regular built-ins, functions and external commands are made in the shell (as in dash), so a
  read-only variable is an error of the shell. Tests: `exec/readonly_assign.sh`.
- Command cache: (file, index in `PATH`), trusted without a stat; `with_command_path` retries later `PATH` entries on
  ENOENT (dash's `shellexec`). `cd` drops entries from relative directories. A search makes one `stat` per directory
  and an `access` only for a regular file. The shell looks commands up before forking, so the cache lasts and a
  missing command costs no fork; in a pipeline, each simple command whose name is a literal word is looked up (dash
  remembers them through `vfork`). Interactive shells `stat` the `PATH` directories after each line and clear the
  cache if one changed (device, inode or mtime). Tests: `exec/path_cache.sh`, `exec/hash_stale.sh`,
  `exec/hash_temp_path.sh`, `builtins/hash_pipeline.sh`, `path_cache` in `tests/interactive.rs`.
- `posix_spawn` (glibc uses `clone(CLONE_VM|CLONE_VFORK)`) for simple foreground external commands when
  `can_spawn()`: not interactive, no job control. `vfork` itself can't be called safely from Rust (the child would
  share the parent's stack). glibc's `posix_spawn` doesn't fall back to `/bin/sh` on ENOEXEC; `spawn_argv` retries
  with this shell. A child under job control must call `tcsetpgrp` itself (else it can get SIGTTIN first); glibc
  2.35+ has `posix_spawn_file_actions_addtcsetpgrp_np` if interactive shells ever spawn. Test: `exec/spawn.sh`.
- The `exit` flag (`run_list_exit`, dash's `EV_EXIT`) is passed to the last element of lists and and-or lists, to
  `if`/`case` bodies and brace groups without redirections, but not to loops, negated pipelines, or compound
  commands with redirections (as in dash). So `$!` is the command itself. Tests: `exec/exec_last.sh`,
  `exec/c_exec_last.sh`, `exec/async_pid.sh`.
- Signals are blocked across `fork` while a signal is trapped or ignored, or the shell is interactive or doing job
  control, until the child has reset its dispositions: a signal sent right after `fork` (`sleep &` then `kill %2`)
  used to be lost.
- Redirections: `n>&n` does nothing even if `n` is closed; `>&word` with a word that isn't a number or `-` is a fatal
  syntax error, as in dash. Tests: `exec/redirect_dup.sh`, `exec/redirect_big_fd.sh`.
- Exec errors other than EACCES give 127, as in Debian's dash (e.g. `ELOOP`). `wait_for` prints a message for deaths
  by signal except INT and PIPE.
- `set -e`: as in dash, only simple commands, subshells and pipelines (and a compound command whose redirection
  fails) exit on their own status, so `{ false && true; }` doesn't exit. Inside `$(...)` the suppression is reset.
  Tests: `errexit/compound.sh`, `errexit/cmdsubst_condition.sh`, `errexit/*`.
- A function can't be named after a special built-in ("Bad function name").

### Jobs (`jobs.rs`, `builtins/jobs.rs`)

- Without job control only background jobs are recorded, with no command text (as in dash); `JobTable::reclaim`
  imitates dash's `makejob`: making a new job frees the first finished job that `wait` has reported. `fork_child`
  calls it for the first process of each job.
- `wait_foreground`: without job control it waits for each pid directly; with job control it records a job and uses
  `wait_job`, which reaps any child (`waitpid(-1)`) until the job stops or ends.
- Under job control: `setpgid` in parent and child, `WUNTRACED`, a job killed by SIGINT makes the shell act as though
  it got the SIGINT (as dash). After a job exits normally the terminal modes are kept (so `stty` works); after it
  stops or dies from a signal they are restored (as bash; dash doesn't).
- Job text follows dash's `cmdtxt` (`$x` becomes `${x}`, single quotes become double quotes, `$(...)` elided,
  assignments dropped). The stopped-jobs warning only lets you out with an *immediately* repeated `exit`.
- `wait` uses dash's statuses (127 for an unknown pid, 2 for an unknown job; only a pipeline's last pid names it).
  `kill` is a port of dash's; signal names follow dash's table (any case, no `SIG`, `RTMIN+n`/`RTMAX-n`, no name for
  16), also for `trap`, which takes no options. Tests: `builtins/jobs.sh`, `builtins/kill_job.sh`,
  `builtins/kill_trap_signals.sh`, `tests/interactive.rs`, unit tests in `cmdtext.rs`.

### Built-ins (`builtins/`)

- Ported from dash nearly verbatim, so compare with dash's source before "fixing": `test`'s parser, `getopts`,
  `umask`, `ulimit`, `kill`, `describe_command` (`command -v`/`-V`, `type`), `single_quote` (output of `set`,
  `export -p`, `alias`, `trap`), `number()` (strtoimax, 0..INT_MAX, for `exit`, `return`, `shift`, `kill`, `wait`).
  Tests: `builtins/test_parse.sh` (every expression of up to four arguments from a set of tokens),
  `builtins/getopts_dash.sh`, `builtins/umask_modes.sh`, `builtins/ulimit_dash.sh`, `builtins/command_describe.sh`,
  `builtins/quoting_output.sh`.
- `echo`: `-n` only, XSI escapes always, dash's `\0nnn` and `\nnn`, and Debian's `\e`. `printf`: numeric conversions
  through libc `snprintf`, unsigned ones through `strtoull` (`-1` wraps), `strerror(ERANGE)` for out-of-range, status
  2 for an invalid directive, no options. Test: `builtins/printf_escapes.sh`.
- `getopts`: `OPTIND` moves past an argument as soon as its first letter is read, `OPTARG` is left alone at the end,
  the position is reset by assigning `OPTIND`, `set --` and `shift`, and saved across function calls; `OPTIND` must
  be a number.
- A built-in whose output can't be written prints `name: I/O error` and its status gets bit 1 (dash's `evalbltin`).
  Test: `builtins/write_errors.sh`.
- `set -x` doesn't trace commands run while `PS4` is expanded (`in_ps4`, dash's `inps4`), which used to loop forever.
  Test: `options/xtrace_ps4_subst.sh`.
- `export`, `readonly`, `local` and `setopt` expand assignment-like arguments as assignments, as dash 0.5.12 does,
  also through `command` and when the name comes from an expansion (`declaration_command` in `expand/mod.rs`). Test:
  `builtins/declaration_args.sh`.
- `cd` and `pwd` use the logical directory (dash's `curdir`); a valid `$PWD` at startup is used without `getcwd`. Tests:
  `builtins/cd_logical.sh`, `builtins/chdir.sh`, `builtins/cd_e.sh`.
- `cd.auto` (`autocd_target` in `exec/simple.rs`) costs nothing unless the option is on. Test: `builtins/autocd.sh`.
- Directory stack (`dirstack.rs`, `Shell::dirstack`): `pushd` goes through `cd`'s code (`CDPATH`, `PWD`, `OLDPWD`,
  `chpwd`), but `-P` wins over `-L` as in zsh; only `-q`, `-L` and `-P` are options. Not implemented: zsh's
  `PUSHD_MINUS`, `PUSHD_TO_HOME`, `DIRSTACKSIZE`, `cd +n` without `pushd.auto`, the `dirstack` array. Tests:
  `builtins/dirstack.sh`, `builtins/dirstack_interactive.sh`, `builtins/auto_pushd.sh`, `builtins/pushd_silent.sh`,
  `builtins/popd_dir.sh`.
- `unset` of a bad name is an error; `set -` turns off `-x` and `-v`; `.` of a directory reads nothing. Test:
  `builtins/special_misc.sh`. `source`: `builtins/source.sh`, `builtins/source_missing.sh`.
- `__luish_internal` (`internal.rs`) holds luish's own commands, so they don't take names from the command
  namespace; a missing or unknown subcommand is status 2. `print-git-rev` is set at compile time by `build.rs`
  (`-dirty` if `src/`, `build.rs`, `Cargo.toml` or `Cargo.lock` differ). Test: `builtins/internal_git_rev.sh`.
- `savestate` (`state.rs`): not `PPID`, `LINENO`, or the options `-i -s -m -n`. Functions are printed by
  `unparse.rs`, which keeps all quoting (unlike `cmdtext.rs`); words in function bodies that would be expanded as
  aliases (command names that are aliases of any kind, other words that are global aliases) are quoted, and a
  function named like an alias is preceded by `unalias`. Loaded plugins are printed as
  `__luish_internal plugin restore NAME PATH`, after aliases and before options. When there are aliases, the commands
  from them on are grouped in `{ }` (`join`), which is parsed before any of it runs, so that a global alias doesn't
  change the words after it (`set -o NAME` ...). Tests: `builtins/internal_savestate.sh` (a new shell reading the
  state prints the same state), `builtins/internal_savestate_aliases.sh`, unit tests in `unparse.rs`.
- `help` (`help.rs`) shows the Markdown in `docs/builtins/` (compiled in with `include_str!`). It is a built-in only
  in shells started with `-i` (which `set` can't change, so also their subshells). Unit tests check that every
  built-in has a page, that pages fit 80 columns and that `docs/builtins.md` includes them all. Tests:
  `builtins/help_noninteractive.sh`, `builtins/internal_help.sh`, `help_builtin` in `tests/interactive.rs`.
- `local x` keeps the current value, as in dash.

### Options (`options.rs`)

- dash's table order; `$-` lists letters in reverse table order. luish's own options have no letter and aren't in
  `set -o` or `$-`. An unknown option, or `interactive` and `stdin`, is status 1 (the other names are still set).
- `setopt -p GROUP`: an unknown group is status 1 (nothing set), a missing one or another option letter status 2. The
  completer completes group names after `-p`, and the group's names after `-p GROUP`.
- Tests: `options/setopt.sh` (zsh), `options/setopt_list.sh`, `options/setopt_values.sh`, `options/setopt_group.sh`,
  `options/*`.

### Interactive mode (`interactive/`)

- **History** (`history.rs`) implements rustyline's `History` trait (its `FileHistory` can't remove an entry or keep
  stable numbers). `add_current` marks the newest entry as the running command, also when it was a duplicate and not
  added, so `fc` leaves it out; `fc -s`/`-e` call `remove_current` and add what they run. `interactive::with_history`
  borrows the editor's `RefCell`: never run commands inside its closure. `fc` re-running `fc` is limited to 4 levels
  (dash's `MAXHISTLOOPS`). The reference for `fc` is upstream dash's `histedit.c` (`histcmd`, `str_to_event`) plus
  POSIX; bash was used to check how the `fc` entry itself is treated.
- **History file** (`histfile.rs`): zsh's extended format (metafied; `\` before embedded newlines and a space after a
  final `\`), and luish's old `#V2` is still read. Appends under an `fcntl` lock (mode 0600, creating the directory);
  trims through a temporary file past 20% over `SAVEHIST`. With `history.share`, one `stat` before each prompt when
  nothing changed: the shell remembers the file's size and last entry seen, which it looks for if the file was
  replaced. It is read after the startup files, so they can set `HISTFILE`/`HISTSIZE`. Tests: `history_file` and
  `share_history` in `tests/interactive.rs`, unit tests in `histfile.rs` (including lines written by zsh),
  `builtins/fc_noninteractive.sh`, `fc_history` in `tests/interactive.rs`.
- **Prompts** (`prompt.rs`): the expansion keeps escape sequences apart from the text, and gives rustyline both (its
  `(raw, styled)` prompt), so the cursor position doesn't count them. Nothing is done unless the option is on and the
  prompt has a `%`. Tests: `misc/prompt_percent.sh` (checked against zsh while written; zsh can't be the reference
  because its interactive mode writes more than the prompts), `misc/prompt_percent_long.sh`, `prompt_percent` in
  `tests/interactive.rs`, unit tests in `prompt.rs`.
- **Key bindings** (`keys.rs`): luish's keymap (zsh's widget names and emacs bindings) comes before rustyline's. Keys
  are decoded as rustyline decodes xterm's sequences, so `^[OA` and `^[[A` are the same key. rustyline overwrites the
  count of any `Move`/`Kill` a handler returns with the numeric argument (`cmd.redo(Some(n))`), so only count-1
  searches work; `word_cmd` picks a rustyline motion that lands in the right place (so kills go to the kill ring),
  else edits through `Completer::update` (no kill ring). Widgets that need the history go through the completer
  (`Pending`) or the hinter (which records `history_index`). `history.rs::starts_with` implements the prefix search,
  sharing `Search` with the key handlers. Bindings changed with `bindkey` are part of the saved state. Keys given
  by name (`named`: `Up`, `Ctrl-X Ctrl-E`) are turned into the bytes xterm sends and decoded as the others; an
  argument is read as names only if every space-separated word is a name, has a modifier, or is one character (and
  some word isn't just a character), and `show` writes the first character in octal when its output would read as
  names (`\125p` for U, p), so listings and saved state read back as the same keys. Tests:
  `line_editor_keys` in `tests/interactive.rs`, `builtins/internal_bindkey.sh`, `builtins/bindkey.sh`, unit tests in
  `keys.rs` and `history.rs`.
- **Completion** (`complete.rs`): a rough tokenizer finds command position (quotes, operators, redirections,
  assignments, `$(`, backquotes, reserved words, commands such as `sudo` that take a command) and records where each
  unquoted byte ends in the line, so a non-prefix match can replace the text from where it stops matching. `PATH`
  executables are cached until `PATH` or one of its directories changes. rustyline's own listing is never used:
  `Completer::complete` returns 0 or 1 candidates (in `CompletionType::List`, one candidate is put in the line with
  `update`).
- **Completion menu** (`menu.rs`): drawn as rustyline's **hint** (a multi-line string starting with `\n`; rustyline
  includes it in its layout, skips SGR escapes when measuring, and erases it on accept), so rustyline isn't patched.
  `Hint::completion()` returns None so Right doesn't insert it. Menu keys are `ConditionalEventHandler`s that set
  `Menu::pending` and return `Cmd::Complete`; `Menu::step` makes the move, and the new text is the only candidate, so
  every change goes through rustyline's completion and undo. The menu is "open" only while `(line, pos)` equals what
  it last put there, so any other edit closes it. Handlers must be `Send + Sync`, hence `Arc<Mutex<Menu>>`; don't
  hold the lock while a plugin completer runs. Column widths are capped at the 90th percentile or a third of the
  screen. rustyline's default `keyseq_timeout` is None (a lone Esc waits for the next key); luish sets 400 ms in
  emacs mode (zsh's `KEYTIMEOUT`) and 100 ms in vi mode.
- **Highlighting** (`highlight.rs`): command lookups are cached until the next prompt; `$LUISH_HIGHLIGHT` and
  `$NO_COLOR` are read before each prompt.
- **Autosuggestions**: the hint while the cursor is at the end of a non-blank, non-continuation line and the menu
  isn't open; accepted with rustyline's `CompleteHint`. The search goes from the newest entry and stops at the first
  match. Test: `autosuggestions` in `tests/interactive.rs`.
- Tests: `tests/interactive.rs` and unit tests in `complete.rs`, `menu.rs`, `keys.rs`, `highlight.rs`,
  `history.rs` and `histfile.rs`.

### Startup files (`main.rs`, `startcache.rs`, `config.rs`)

- Order: `config.toml`, `rc.d`, then the login files (`login.d`, or else `/etc/profile` and `~/.profile`), `$ENV`,
  `luishrc`. Login files run for login shells whether interactive or not, as in dash.
- The cache is the difference between the state (`state.rs`) before and after the files ran, as commands, so
  inherited variables that the files don't touch aren't saved (`difference()` over keyed `Entry`s;
  `Shell::sourced_files` records `.` paths during a build). The key is the build of luish (the git revision, plus a
  hash of the sources for a dirty build; see `build.rs`), the directory, the list of files, and the fingerprint
  (device, inode, size, mtime) of each file and of every file they read with `.`; `rc.d`'s key also covers
  `config.toml`, even when it doesn't exist. Written with a rename, mode 0600; the directory gets a `CACHEDIR.TAG`
  and a `README`. On the warm path: a stat per file, a directory read and one file read per directory. Test:
  `misc/startup_cache.sh`.
- Not yet done (see `PLAN.md`, Stage 3): keying on the inherited values the files read (so `PATH=$HOME/bin:$PATH`
  keeps the rest of `PATH` from when the cache was built, and an `rc.d` cache built in a login shell, before
  `login.d` ran, is used in shells started from it), changes a fingerprint can't show, per-file entries, background
  revalidation, `flock` for many shells at once, and merging into running shells.
- `config.toml` is parsed with `toml-span`; errors are `luish: PATH: line N: ...`, in the file's order. A key directly
  under `[options]` is a setting by its `setopt` name. The `alias` table defines regular aliases, and its `global` and
  `suffix` tables the other kinds (so a string named `global` or `suffix` is a regular alias, and TOML won't have both
  in one file). The `bindkey` table goes through `keys::bind_widget`, as `bindkey KEY WIDGET` does. A directory
  plugin's `plugin.toml` shares the `options`, `alias` and `bindkey` tables (`config::load_plugin_manifest`, see
  Plugins). Test: `misc/config_toml.sh`.
- The rc stage (`interactive::rc_d`, `startcache::run` with `config`) is: `config.toml`'s options, the plugins it
  enables (`plugins::load_enabled`), `rc.d`'s files, then every loaded plugin's `post-rc.lsh` (`post_rc_files`), all
  inside the cache; then, outside it and so in every shell, the `post-rc` hooks (`post_rc_hooks`), then
  `_uncached.lsh`. `Shell::in_rc` is set meanwhile, so that `plugin load` defers `post-rc.lsh` and hooks; outside
  it, a plugin runs them right after `rc.lsh`. With `config.toml` but no `rc.d`, the cache is still used (for the
  nonexistent directory). A startup with plugins that aren't installed or can't be resolved doesn't write the cache,
  so the message repeats until they are. `--no-plugins` bypasses the caches (reading one would restore plugins'
  effects, and writing one would save a state without them). Tests: `tests/plugins/packages.sh`,
  `tests/plugins/post_rc.sh`.

### Plugins (`plugins/`)

- Until the first `plugin load`, the only state is `Shell::plugins` (`None`) and the only cost is the `None` check in
  `cd`. A plugin without an extension doesn't create the Rhai engine. Without the feature, `plugin load` fails with
  "luish was built without plugin support". CI also runs clippy and the tests with `--no-default-features`.
- `plugins/rhai.rs`: one `Engine` (with call-depth, expression-depth and size limits), one AST per extension, so
  helpers with the same name in different extensions don't clash. `import` resolves relative to the plugin's
  directory; imported modules are cached until a plugin is loaded again. SIGINT stops extension code (checked in
  `on_progress`), leaving the signal pending for the shell. Rhai installs no signal handlers and has no threads or
  buffered output, so nothing happens around `fork` (built without its `sync` feature). Release builds use
  `panic = "abort"`; a panic in Rhai is a Rhai bug to report, not something to `catch_unwind`.
- Rhai is built without default features (whose `runtime-rng` pulls in `libdl`) and with `only_i64`, which roughly
  halves its load-time relocations. Floats are kept, although they make the executable depend on `libm`.
- The `sh` functions reach `Shell` through a pointer set for the length of each call into Rhai. Calls are re-entrant
  (`sh::run`), so no `&mut Shell` borrow can be held across a call into Rhai or back into the shell.
- Completers run with `Shell::jobctl` taken out (so their commands aren't jobs and don't save the editor's raw modes)
  and `$?` kept; they are stopped after 2 s (checked every 1024 operations). An error is printed and the line redrawn
  by returning the word itself as the only candidate. `fc` sees no history while a completer runs (the editor is
  borrowed).
- `prompt-vars`: the variables the step changed are found by comparing with a snapshot of all variables
  (`Vars::changes_since`) and put back after the prompt is built; no snapshot is taken if nothing has a
  `prompt-vars` hook or file. `prompt-rewrite` hooks that take a parameter are found by looking up the function in
  the extension's AST when it is registered.
- **Bytes** (`bytes.rs`), as Python's `surrogateescape` (PEP 383), but Rust strings can't hold lone surrogates, so
  each byte `b` of an invalid UTF-8 sequence becomes U+10FF00 + `b` (bytes 0x80–0xFF map to U+10FF80–U+10FFFF), and
  back. Values round-trip exactly, except that real U+10FF80–U+10FFFF characters in shell data become raw bytes; that
  range is effectively unused (Nerd Fonts use the BMP private-use area and plane 15). Displayed text shows escaped
  bytes as U+FFFD; completion candidates with escaped bytes are dropped; strings with NUL can't be set as variables.
- `vcs.rs` reads `.git` without forking (`HEAD`, loose and packed refs, `commondir` for worktrees, `vcs_info`'s action
  names, the stash log), running git only for the reftable format and for `vcs::status` (`git --no-optional-locks
  status --porcelain=v2 --branch -z`). Not supported: bare repositories, `GIT_DIR`, `GIT_CEILING_DIRECTORIES`.
- Examples: `docs/examples/cobra.rhai` (programs built with Cobra). Plugins for use, in the collection
  `luish-std-plugins/` (to become a repository of its own; the source `std`): `git-completion.rhai`
  (lists commands from `LC_ALL=C git help -a` without the low-level and guide sections, options from
  `git CMD --git-completion-helper`, files from `ls-files`/`diff --cached`, collapsed to the next directory) and
  `bash-completion/` (a default completer that runs bash-completion in bash through `bridge.bash`; about 50 ms per
  Tab, since bash sources `bash_completion` each time). Outside bash's own completion, compgen doesn't undo
  the quoting bash-completion gives the word (`~` as `\~`), so the bridge replaces the quoting functions; its
  `-o` options are in `copts`, since completion functions have a local `opts`.
- **Packages** (`package.rs`, `fetch.rs`): `read_config` turns `[plugins]` into owned `Config` (sources in
  `plugins.available`, plus the built-in `std`; entries in `plugins.enabled`), and `manifest` a directory plugin's
  `plugin.toml` into entries of the same kind. `Resolver` resolves entries depth-first, dependencies before
  dependents, identifying plugins by absolute path: the same plugin twice is loaded once, two plugins with one name
  and a cycle (found on the stack) are errors, and a failed dependency fails its dependents. A plain `NAME` in a
  manifest is looked for in the collection the plugin came from (`Scope::Collection`). Git sources resolve through
  pins (URL and ref to commit): pins already used in this run, then (unless updating) `plugins.lock`, then (only for
  `sync`/`update`) `fetch::resolve`. `plugin sync` also resolves every plugin of each `plugins.available` source (each
  separately, so unrelated name clashes don't matter), so their pins are locked too; problems there are reported but
  don't stop the lock being written, while problems with enabled plugins do. The lock is written only if its text
  changed (so the rc cache, which fingerprints it, stays valid). Messages about manifests of git plugins show
  `SOURCE:PATH/plugin.toml` rather than the data directory.
- A loaded plugin is named after its file or directory (`std/git-completion` loads as `git-completion`), so
  `plugin list-available` leaves out the loaded plugins by absolute path, and `plugin unload ARG`, if no plugin is
  loaded under the name ARG, unloads the one at the path that `plugin load ARG` would load (`package::location`,
  else `find`). Test: `tests/plugins/packages.sh`.
- `plugin.toml`'s `options`, `alias` and `bindkey` tables are applied by `load_found` (with `config.rs`'s code), in
  interactive shells, after the extension loads and before `rc.lsh`; its options override `config.toml`'s, by design
  (a plugin can package a set of options). The file is parsed again there (the resolver only keeps the
  dependencies), and syntax errors are left to the resolver, so they are reported once. `load_found` records the
  file for the rc cache, also for plugins that `rc.d` loads with `plugin load`. Test: `tests/plugins/manifest.sh`.
- `fetch.rs` runs git through the shell (`command git`, in a forked child, with `GIT_TERMINAL_PROMPT=0`), with
  `-C` a bare repository per URL in `$XDG_CACHE_HOME/luish/plugins/git/REPO-HASH` (FNV-1a of the URL). A ref is
  fetched with `--depth 1` and read from `FETCH_HEAD^{commit}`; a locked commit that is missing is fetched by hash,
  else with a full fetch of its ref. `git archive` into a temporary directory next to `src/REPO-HASH/COMMIT`,
  extracted with `tar` and renamed into place, so an existing directory is complete. The data directory (not the
  cache) holds these, since startup needs them and can't recreate them, with a `README` saying that `plugin sync`
  fetches them again; the bare repositories are only for fetching, so they are in the cache (whose `README` lists
  them).
- Not yet done (see `PLAN.md`): `plugin add`/`remove`/`gc`, version requirements other than `"*"`, `flock` for
  concurrent syncs, `login.lsh`.
- Tests: `tests/plugins/*` (packages: `packages.sh` for local sources, `git_packages.sh` for git ones with
  `file://` repositories, `post_rc.sh`, `manifest.sh`), `builtins/plugin.sh`, `builtins/internal_plugin.sh`, unit
  tests for the byte conversion, `git status` parsing and (with a stand-in completer) in `complete.rs`, and
  `plugin_builtin`, `plugin_completer`, `cobra_completer`, `git_completion` and `bash_completion_bridge` (skipped
  without bash-completion) in `tests/interactive.rs`.

### Signals and startup

- `main` is a C `main`, so Rust's runtime set-up doesn't run: SIGPIPE stays as inherited. Signal handlers are
  installed without `SA_RESTART` so that `wait` gets EINTR; all syscall wrappers in `sys.rs` retry on EINTR
  (`sys::read` has an `interruptible` flag).

## Deviations and their tests

Each deviation in `docs/compatibility.md` has a test: a case marked `# reference: zsh`, or one with a `.expected`
file. When adding a deviation, add it to both.

| Deviation | Tests |
|---|---|
| `source` | `builtins/source.sh` (zsh), `builtins/source_missing.sh` |
| `pushd`, `popd`, `dirs` | `builtins/dirstack.sh` (zsh `-o noposixcd`), `builtins/dirstack_interactive.sh` (zsh), `builtins/popd_dir.sh` |
| `setopt`, `unsetopt` | `options/setopt.sh` (zsh), `options/setopt_list.sh` |
| `%` sequences in prompts | `misc/prompt_percent.sh`, `misc/prompt_percent_long.sh` |
| `**/` | `expand/globstar.sh` (zsh), `expand/globstar_off.sh` (dash), `expand/globstar_loop.sh` |
| Glob qualifiers | `expand/glob_qualifiers.sh`, `expand/glob_qualifier_errors.sh` (zsh `+o shglob -o bareglobqual +o ksharrays`), `builtins/internal_savestate_globqual.sh` |
| A directory as a command | `builtins/autocd.sh` (zsh) |
| `bindkey` | `builtins/bindkey.sh` (same as dash), `builtins/internal_bindkey.sh`, `line_editor_keys` in `tests/interactive.rs` |
| History file | `history_file` and `share_history` in `tests/interactive.rs`, unit tests in `interactive/histfile.rs` |
| `alias`, `unalias` options | `builtins/alias_options.sh` (zsh), `builtins/alias_deviations.sh` |
| Global aliases | `parse/alias_global.sh` (zsh), `builtins/alias_deviations.sh` (here-document delimiter), `builtins/internal_savestate_aliases.sh` |
| Suffix aliases | `parse/alias_suffix.sh` (zsh), `builtins/alias_deviations.sh` (`command -v`) |
| Last command of `sh -c` | `exec/c_exec_last.sh` (zsh) |
| Script read from a pipe | `misc/stdin_script.sh` (zsh) |
| `$LINENO` | `misc/lineno.sh` |
| fd numbers in redirections | `exec/redirect_big_fd.sh` |
| `exec -- cmd` | `exec/exec_dashdash.sh` |
| `cd -e` | `builtins/cd_e.sh` |
| `set -o` / `set +o` list | `options/set_o_hashall.sh` |
| `set -x` output | `options/xtrace.sh` |
| `kill %n` without job control | `builtins/kill_job.sh` |
| `fc` | `builtins/fc_noninteractive.sh`, `fc_history` in `tests/interactive.rs` |
| `$((` fallback | `parse/arith_fallback.sh` |
| `emacs` option | `options/interactive_c.sh` |
| Command cache | `path_cache` in `tests/interactive.rs` |
| Command-line options | `options/command_line.sh` |
| `__luish_internal` | `builtins/internal_savestate.sh`, `builtins/internal_git_rev.sh` |
| Startup files | `misc/startup_cache.sh`, `misc/config_toml.sh` |
| Grouped option names | `options/setopt_values.sh`, `options/setopt_group.sh`, `options/setopt_list.sh` |
| `help` | `builtins/internal_help.sh`, `builtins/help_noninteractive.sh` (same as dash), `help_builtin` in `tests/interactive.rs` |
| `plugin` | `builtins/internal_plugin.sh`, `builtins/plugin.sh` (same as dash), `plugin_builtin` in `tests/interactive.rs` |

## dash as the reference

- The system dash is Debian's 0.5.12 with patches. Notably it doesn't exec the last command of `sh -c` (Debian patch
  0004), processes `\e` in `echo`/`printf`, and gives 127 for exec errors other than EACCES. Upstream source:
  `https://git.kernel.org/pub/scm/utils/dash/dash.git/plain/src/<file>?h=v0.5.12` (sometimes returns 502; retry).
  Debian's patches: `https://sources.debian.org/api/src/dash/0.5.12-12/debian/patches/`.
- dash behaviours the tests rely on: `$-` lists option letters in reverse table order; `$(...)` doesn't update `$?`
  in the middle of a command; `set -x` doesn't quote; there is no `$LINENO` and no `-h`; `.*` matches `.` and `..`;
  alias listing is in hash order, so tests list aliases by name; without job control, jobs have no command text.

## Testing notes

- pty tests use `TERM=dumb` (rustyline does no editing), except completion and highlighting, which use `TERM=vt100`.
  After Ctrl-C, wait for `"\x1b[K$ "` (the fresh prompt), not `"$ "`, which also matches the line redrawn under the
  completion menu. Ctrl-C at the prompt sets `$?` to 130.
- A job notification is printed before the *next* prompt, so it shows up in the output of the command that caused
  it. With `stty -echo`, neither the command nor its newline is echoed; Ctrl-D isn't echoed as a newline.
- The pty is created with size 0x0 (rustyline then assumes 80 columns, the menu 24 rows); `Pty::resize` sets it.
- Checking the line editor by hand: tmux works well (`tmux new-session -d -x 70 -y 10 ...`, `send-keys`,
  `capture-pane -p [-e]`), but run `tmux set -sg escape-time 0` first, or tmux holds Esc for 500 ms and glues it to
  the next key. Wait after Enter before typing: input sent before the next prompt is discarded.
- pixi task `outputs` caching ignores paths under `.pixi/`.

## Conformance

Checked on 2026-09-26 (the scripts are not in the repository):

- **autoconf**: GNU hello 2.12.1 and GNU sed 4.9 `configure` give the same output and `config.h` under luish (as
  `CONFIG_SHELL`) as under dash; both build and `make check` passes (sed: the same PASS/SKIP lists). To rerun: get
  them from ftp.gnu.org, run `CONFIG_SHELL=$L $L ./configure` next to a dash-configured copy, and diff the output,
  `config.h` and `make check` results.
- **Oils spec tests** (`spec/*.test.sh` whose `compare_shells` include dash, 1620 cases): 161 differed at first, 43
  now: the deviations, bash-only features (arrays, `shopt`, `printf -v`/`%q`, `declare`), cases that differ only by
  temporary directory names or timestamps, and the known limitations. To rerun: `git clone --depth 1
  https://github.com/oils-for-unix/oils`, split `spec/*.test.sh` on `#### ` (skipping `## STDOUT:`...`## END`
  blocks, which are expected output), keep files whose `## compare_shells:` includes dash, and run each case with
  `sh -c` under dash and luish in a fresh temporary directory (env: `PATH`, `SH`, `TMP`, `HOME`, `REPO_ROOT`,
  `LC_ALL=C.UTF-8`; stdin `/dev/null`; timeout 5 s), comparing stdout and status. `case $SH in dash)` in the cases
  doesn't match when `SH` is a full path, so some cases meant to skip dash run anyway.

## Performance

The timings (script benchmarks against dash, bash, zsh and BusyBox, startup, and the startup cache) are in the user
docs, `docs/performance.md`; update them there after rerunning `bench/run.sh` or the startup measurements. What
follows is what they came from and what is left.

Per external command, luish makes the same syscalls as dash (before `posix_spawn`, a loop running `/bin/true` 3000
times took 2.42 s, now 1.80 s as in dash). Startup makes 56 syscalls to dash's 49 (it made 140 before `#![no_main]`,
lazy signal-disposition lookup, and looking up the executable's path only when a script without `#!` needs it). The
remaining startup gap (about 1.5 ms to dash's 1.15 ms per `-c :` from a loop) is the dynamic loader: relocating a 3 MB
binary and loading `libm` (for Rhai's floats) and `libgcc_s`. The `plugins` feature accounts for about 250 µs,
accepted while it stays under 1 ms. A build without it takes 1.34 ms, and a static one
(`-C target-feature=+crt-static`) 1.09 ms, but static glibc looks users up (`~user`) through NSS modules loaded at
run time.

Profiled with callgrind, the in-shell gap in the script benchmarks came from `$((...))` comparing the text with each
of 35 operator strings, SipHash on every variable lookup, `${x#pat}` trying every prefix or suffix (and copying),
`case` compiling literal patterns, and needless copies. Work inside the shell is now as fast as dash or faster; the
fork-heavy scripts are within 4%, mostly startup. Most of the remaining in-shell time is `malloc` and `free`, since
expansion builds `Vec`s where dash uses its stack allocator.

luish parses the warm startup cache about twice as slowly as dash parses the same file (the `-n` row of the startup
cache table), which is worth profiling (and see lazy function parsing in `PLAN.md`).

## Known gaps for developers

User-visible limitations are listed in `docs/compatibility.md`. Beyond those:

- Fds saved at 10 or above could collide with a user redirection to fd 10+ in the same command.
- The native built-ins are a `fn` table, not yet on a `Builtin` trait shared with extension built-ins.
- No fuzz targets (lexer, parser, arithmetic, pattern matcher) and no `insta` snapshots.

## References

- POSIX.1-2017, XCU chapter 2 "Shell Command Language", and the pages for `sh`, `set`, `trap`, `read`, `test`,
  `printf` and `getopts`.
- The source code of dash; mrsh; the Oils project's blog posts on shell parsing.
- The Rhai book (<https://rhai.rs/book>), especially embedding, safety limits, function pointers and closures.
- The glibc manual's chapter "Implementing a Job Control Shell".
