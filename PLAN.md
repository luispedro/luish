# luish — Implementation Plan

`GOALS.md` groups the goals into three stages. This plan covers what is still to be built: the rest of Stage 1, the
plugin system from Stage 2, and sketches of the later stages with the constraints they put on the design now. What
is already built, and how, is in `DEVELOPING.md`; the user documentation is in `docs/`.

## Status (2026-10-05)

Phases 0 to 10 (the POSIX shell and the interactive mode) are done. The open phases:

| Phase | Stage | Status |
|---|---|---|
| 11 Plugins | 2 | Started, because the daily driver's prompt and completion build on it |
| 12 Conformance and performance | 1 | Under way |
| 13 Replacing zsh | 1 | **Current focus** |

### Next steps, in priority order

1. **Phase 13, in its order**: the rest of the generic completion bridge (needs a design note first) and typing to
   narrow the menu; lazy function parsing of the startup cache.
2. **Startup cost** (deferred by the user for now; see Performance in `DEVELOPING.md`): the dynamic loader's share,
   and parsing large files (the rc cache, `nvm.sh`) about three times as slowly as dash.
3. **More conformance**: larger `configure` scripts (coreutils), other Oils files that don't list dash, the smoosh
   and modernish suites.
4. **Completion polish** beyond Phase 13: group headings (as zsh's commands / files / ..., which needs a group on
   `Candidate`, also for plugins), `LS_COLORS` for files, fuzzy matching.
5. **Plugins (Phase 11)**: time budgets for hooks, `plugin remove`, the example plugins of M5.

## Phase 11 — Plugin system (Stage 2)

1. A plugin-agnostic `Builtin` trait, with the Rust built-ins moved onto it, so that extension and native built-ins
   go through the same code path (extension built-ins are now a separate `CommandKind::Extension`):

   ```rust
   pub trait Builtin {
       fn name(&self) -> &[u8];
       fn special(&self) -> bool { false }   // extensions can never register special built-ins
       fn run(&self, sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult;
   }
   ```

2. The rest of the `sh` module:

   | Function | Description |
   |---|---|
   | `argv0()`, `positional()` | `$0`, and `$1...` as an array |
   | `chdir(path)` | Change the directory as `cd` would, updating `PWD` and running `chpwd` hooks |

3. Time budgets (checked in `on_progress`) for every hook that runs while the user waits (`precmd`, `preexec`,
   `prompt-vars`, `prompt-rewrite`, `chpwd`), not only completers. A hook over its budget is reported and skipped.
4. Example plugins: a git-aware prompt, a `json` query built-in, and a command-timing `preexec`/`precmd` pair (M5).
5. Plugin packages, below.

### Plugin packages

1. `plugin remove NAME...`, editing `config.toml` as text as `plugin add` does, and `plugin gc` for the repositories
   and checkouts the lock doesn't use (`sync` and `update` never remove them, since a running shell may still read
   their files).
2. `plugin list-loaded -l`: each plugin's source and commit, and the enabled plugins that aren't installed.
3. `login.lsh`: run after `login.d`, in its cache, or uncached after `~/.profile` without `login.d`.
4. `flock` on the data directory, for two `plugin sync` at once (extraction is already safe: rename into place).
5. Version requirements other than `"*"`, once plugins have versions (a `version` in `plugin.toml`, or tags).
6. More in `plugin.toml`: `description` (shown by `list-available -l`), the oldest luish a plugin needs.

**Later**: a plugin's `bin/` on `PATH` and `completions/`; Tab after `plugin load NAME` offering its options (and
`plugin list-loaded -l` showing them); archives for systems without git.

## Phase 12 — Conformance and performance

Still to do: the startup gap (deferred), larger `configure` scripts (coreutils) and distribution scripts, the smoosh
and modernish suites, `insta` snapshot tests for the parser, and `hyperfine` in CI (non-blocking) for startup and
loop regressions, including `-c true` with and without the `plugins` feature. Optimisations only where profiling
shows a need.

## Phase 13 — Replacing zsh (Stage 1, current focus)

Stage 1 asks for a daily driver "at least as good as a basic zsh setup". This phase is what luish still lacks to
replace the author's own zsh setup (`~/.zshrc` from home-manager plus zplug, and `~/.zshrc_local`), found by probing
each item against luish and from about 670 commands of typed history.

Still to build, in this order:

1. **Completion content**, the biggest gap by volume (zsh gets git, ssh, make, man, cargo ... from `compinit` and
   zsh-completions). The `completion` plugin in `luish-std-plugins/` covers git and about 230 common commands; still
   missing:
   - **git**: `REV:PATH`, `git config` keys, and values for most `--option=` words.
   - **Other commands**: paths on remote hosts for scp and rsync, the packages that dnf, yum and zypper can install
     (their lists take seconds; zsh keeps a cache), and options for `xargs` (luish's completer skips it as a
     precommand, so no plugin sees it).
   - **A generic bridge**, so that most programs get completion without a hand-written completer: programs that
     complete themselves (clap's `COMPLETE=`, `argcomplete`, Typer's; Cobra, Click and nix are done),
     bash-completion scripts (as in `luish-std-plugins/bash-completion/`, but faster), and `--help` parsing for
     options only (as fish does). This needs a design note before building.
   - **Typing to narrow the menu** (zsh's `menu select interactive`): while the menu is open, printable keys filter
     the matches instead of closing the menu.
2. **Lazy function parsing for the startup cache**, below.

Not planned, because the history shows they aren't used or they are easy to rewrite in POSIX sh: zsh's
two-argument `cd old new`, `vared`, `zmv`, `mmv`, `zed`, `zcalc`, `noglob`, zsh-history-substring-search, zsh-nvm,
zplug, and `fpath`/`compinit`. Nor are zsh's hook functions (`chpwd`, `precmd` ... as shell functions, and the
`*_functions` arrays): the zsh hook snippets of tools such as direnv, zoxide or mise use other zsh syntax too, and an
extension's hook can call a shell function with `sh::run`. If compatibility is ever wanted, a std plugin can map the
zsh names onto the hooks. The setup's terminal title in `chpwd` becomes a small extension.

The user's config (`~/.config/luish/rc.d`) also needs porting: `setopt prompt.percent` and `PS1`, `CDPATH`,
`setopt cd.auto`, the history settings (`setopt -p history file=~/.histfile size=1000 save_size=1000 share
ignore_space reduce_blanks save_no_dups`), the remaining aliases (`..`, `...`, `ls`, `open`), and the functions
rewritten in POSIX sh. That is configuration, not luish work, but each item above is checked by using it there.

### Lazy function parsing

zsh's `autoload` parses a function's body on its first call. The startup cache saves functions as source text,
parsed again at every startup: on the author's setup this added about 11 ms to a warm start, 9 ms of it parsing nvm
functions that are rarely called (`docs/performance.md` has the timings of a smaller nvm setup).

When replaying the cache, define each function as a stub holding its byte range in the cache text (kept in memory,
not re-read by path, since another shell may replace the cache), and parse the body on its first call. `type`, the
highlighter and the completer need only the names. The deferred parse must reproduce the parser's state at
definition time:

- **Aliases.** The cached text has already been through alias expansion, but an alias defined later (in `luishrc`,
  `_uncached.lsh` or interactively) would be expanded by a lazy parse. Parse deferred bodies with an empty alias map,
  as zsh's `autoload -U` does.
- **`glob.bare_qualifiers`**, the only option the lexer reads. Replay wraps functions with qualifiers in
  `set -o`/`+o`, which works only at parse time. Record the option's value in the stub.
- **Line numbers.** `$LINENO` values are assigned while parsing, so the stub keeps the body's first line.

A deferred parse can fail only if the unparser doesn't round-trip, which is unit-tested and fuzzed; such a bug would
move a syntax error from startup to the first call. Until then, nvm can be loaded lazily, as zsh-nvm's lazy mode
does: put the default node's `bin` on `PATH` and define `nvm() { unset -f nvm; . "$NVM_DIR/nvm.sh"; nvm "$@"; }`.

**Done when:** the author uses luish as the login shell with the ported config, and the history of a week shows no
command that had to be run in zsh.

## Milestones

M1 (simple scripts), M2 (POSIX script engine) and M3 (real-world scripts) are done.

| Milestone | Stage | Phases | Definition of done | Status |
|---|---|---|---|---|
| M4: Daily-driver interactive shell | 1 | 10, 13 | Replaces the author's zsh setup (Phase 13) | Phase 13 in progress |
| M5: Plugins | 2 | 11 | Extension built-ins and hooks; example plugins: a git-aware prompt, a `json` query built-in, command timing | Started |

Stage 1 is complete at M4, plus startup that matches dash (Phase 12). Milestones for the rest of Stage 2 and for
Stage 3 will be planned once Stage 1 is done.

## Later stages

A sketch, not a plan: what the later stages need, so that Stage 1 doesn't make them harder. The constraints on the
design now: all shell state lives in `Shell` (so it can be snapshotted, compared and restored); the line editor gets
only plain data (`Names`), so it could run in another process; all assignments go through one function, and the
shell tracks the current file as well as the line, for provenance and for error messages with a stack.

### Stage 2: beyond POSIX

- **Extensions** stay behind options (as `glob.star`, `glob.bare_qualifiers` and `expand.braces` are), unless their
  syntax is an error in POSIX sh. With them off, POSIX scripts must parse and behave exactly as before and run as
  fast. The lexer checks the options that change parsing in one place (as it does `Parser::bareglobqual`).
- **Arrays**, possible follow-ups, not yet asked for: more parameter flags (`q`/`Q`, `l:n:`/`r:n:`, `#`, `e`; `P`
  would duplicate `${!x}`), flags before `#` (`${(flags)#x}`), nested substitutions (`${${x#a}%b}`), and the
  locale's collation for `(o)`. Not to be done (decided 2026-09-29): zsh's `integer` built-in and base argument
  (`typeset -i 16 x`), `typeset -F`, highlighting subscripts beyond what the highlighter already does, completing
  keys that contain `'`, and `0` rather than `''` for the holes filled in integer arrays.
- **Terminal features**: kitty's `click_events=1` (it moves the cursor with arrow keys, so Up and Down must not
  search the history then), and the kitty keyboard protocol (needs rustyline support). Capabilities can be asked
  (XTVERSION, `CSI ? u`) with the DA1 query that `tty.rs` already sends.
- **Scripting**: a predictable strict mode, and a debugger or step-trace mode.
- **The directory of the current file**, `~.` (decided 2026-10-02 to wait; `${BASH_SOURCE:A:h}` works now): a tilde
  prefix for `${BASH_SOURCE:A:h}`, so `. ~./lib.sh` and `cfg=~./defaults.conf` need no quoting (tilde expansion isn't
  split), in the spirit of `~+` and `~-`. dash leaves `~.` as it is (no user `.`), so this changes behaviour, though
  only for a name that can't be a user. It should take the path made absolute when the file started (joined with
  the current directory then, without a syscall), so that it still works after `cd`, unlike a relative
  `BASH_SOURCE`. Open: in `-c` and on standard input, an error or `~.` left as it is.
- **Variable tracing**, beyond `setopt vars.trace` and `where`: the full call stack of an assignment in a function
  (not only the function running), "eval at FILE:LINE" for `eval`, and for list-like variables (`PATH`, `MANPATH`)
  which components each change added or removed. Inherited variables can only be traced heuristically, by walking
  up the process tree (`/proc/PID/environ` is the environment at exec time, exited ancestors break the chain, other
  users' processes can't be read); a luish started by another luish could receive exact origins through an opt-in
  environment variable.
- **Interactive**: richer completion, and history with metadata (working directory, exit status, duration).
  `history.rs` owns the storage, so it can move to a structured store without changing the editor.

### Stage 3: caching of login scripts

The goal is to cache the *effects* of login scripts. With a warm cache, a new shell should start almost instantly,
and many shells started at once (a desktop login can start 40) should not each run the scripts. The cache of
`rc.d`, `login.d` and `__luish_cache` blocks is built (see `DEVELOPING.md`), with `__luish_internal check-cache` as
a first, manual form of revalidation. The rest of the design is stale-while-revalidate: a shell starts from the
cached state at once, reruns the scripts in the background, and applies any difference at a later prompt, so
invalidation doesn't have to be perfect.

Still to do:

- A background run or `check-cache` warns about a cached block that prints (output happens only when the body runs).
- While checking (so at no cost otherwise), record the inherited variables a block expands without listing them,
  and warn: `rc.d/20-nvm.lsh:2: the block reads SSH_CONNECTION, which is not in its key`.
- `check-cache` checks only entries built in the environment the cache last recorded; recording each build's
  environment (stored once per build, not per entry) would check the others too.
- **If asked for**: `commands=(...)` in `__luish_cache`, the resolved path of each command and its fingerprint, for
  `eval "$(starship init sh)"` and the like without spelling out the path; and a time to live.

#### Later

- **Parallel builds**: each file runs in its own forked child from the input state, and the results are merged in
  byte order. This needs to know which files read what an earlier one wrote, so read tracking during builds, which
  the chain of keys otherwise makes unnecessary. Colon-separated lists would be merged as edits (components added or
  removed), so a read of `PATH` only within an assignment to `PATH` doesn't count as a dependency.
- **Startup**: fork the background run first (from the inherited state); use each matching entry and run what has
  none in the foreground; apply the changes, run the uncached parts, show the prompt.
- **Background run**: stdin from `/dev/null`, its own session, captured output, a timeout; writes entries with
  write-and-rename; skipped if another shell validated the cache recently (a minute) or is doing so now.
- **Many shells at once**: building is guarded by `flock`; a shell that must build in the foreground waits for the
  lock, then checks the cache again. Interactive shells stat the cache files before each prompt and merge newer ones.
- **Merging into a running shell**: a three-way merge of the state the shell loaded, the new result, and the current
  state. What hasn't changed since startup takes the new value, with a one-line note; conflicts are left alone and
  reported, and a built-in shows and applies them. This happens before a prompt, like job notifications.
- **Volatile values** (from `$(date)`, `$$`, `$RANDOM`) differ between two runs with the same key: reported once,
  with the file and line from provenance, and left out of change notes.
- **Parse cache**: parsed ASTs of sourced files, keyed by fingerprint and the alias table, for the uncached parts of
  startup files, `$ENV` and `luishrc`. Functions in the effect cache are parsed lazily (Phase 13).

### Stage 3: SSH client/server mode

The line editor runs on the local client, so typing is instant, while commands run on the remote host.

- **Transport**: standard SSH. The client runs something like `ssh host luish --serve` and speaks a protocol over
  its stdin and stdout. No daemon and no extra ports (unlike mosh's UDP).
- **Line editing mode**: the client shows the prompt the server sends, edits locally, and sends complete command
  lines. Completion and "is this input complete?" are round trips, which is why the editor only sees plain data.
  History can live on the client and be shared across hosts.
- **Pass-through mode**: while a command runs, the server runs it on a pty of its own, and the client forwards
  terminal input and output as-is, plus window-size changes, so full-screen programs work as over SSH today.
- **Open questions**: installing the server binary when the remote host has none; protocol versioning; dropped
  connections (which mosh survives and plain SSH doesn't).

## Risks

| Risk | Mitigation |
|---|---|
| An extension hanging or slowing the prompt | Ctrl-C interrupts extension code; hooks that run before the prompt get a time budget (Phase 11) |
| Few people know Rhai, and it has no library ecosystem | Keep the API small, ship example extensions, provide what extensions need (commands, files, JSON) |
| Scope creep into later stages | No Stage 2 or 3 features beyond what the daily driver needs until M4. Keep extensions that change behaviour behind options |
| Plugin support adding startup cost | Engine created on the first extension; feature flag; benchmark `-c true` with and without it |
