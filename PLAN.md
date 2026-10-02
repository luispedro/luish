# luish — Implementation Plan

`GOALS.md` groups the goals into three stages. This plan covers what is still to be built: the rest of Stage 1, the
plugin system from Stage 2, and sketches of the later stages with the constraints they put on the design now. What
is already built, and how, is in `DEVELOPING.md`; the user documentation is in `docs/`.

## Status (2026-10-01)

| Phase | Stage | Status |
|---|---|---|
| 0 Scaffolding, CI | 1 | Done |
| 1 Minimal REPL, external commands | 1 | Done |
| 2 Lexer | 1 | Done, apart from the fuzz target |
| 3 Parser | 1 | Done, apart from `insta` snapshots and the fuzz target |
| 4 Executor core | 1 | Done |
| 5 Word expansion | 1 | Done |
| 6 Variables and built-ins | 1 | Done |
| 7 Functions, `eval`, `.`, control flow | 1 | Done |
| 8 Signals and traps | 1 | Done, with the limitations in `docs/compatibility.md` |
| 9 Options and `set -e` | 1 | Done |
| 10 Interactive mode and job control | 1 | Done, and beyond the plan: a zsh-style completion menu, syntax highlighting, zsh's `%` prompt sequences, a history file shared with zsh, history expansion, `pushd`/`popd`, `bindkey`, autosuggestions, `**/`, glob qualifiers and a first-run menu |
| 11 Plugins | 2 | Started (below), because the daily driver's prompt and completion build on it |
| 12 Conformance and performance | 1 | Under way (below) |
| 13 Replacing zsh | 1 | **Current focus** (below) |

The first version of the startup cache (Stage 3) is also built, ahead of the plan, since startup files that take a
second (nvm) are otherwise a daily cost.

### Next steps, in priority order

1. **Phase 13, in its order**: the rest of the generic completion bridge (needs a design note first) and typing to
   narrow the menu; lazy function parsing of the startup cache.
2. **Startup cost** (deferred by the user for now; see Performance in `DEVELOPING.md`): the dynamic loader's share,
   and parsing large files (the rc cache, `nvm.sh`) about three times as slowly as dash.
3. **More conformance**: larger `configure` scripts (coreutils), other Oils files that don't list dash, the smoosh
   and modernish suites.
4. **Completion polish** beyond Phase 13: group headings (as zsh's commands / files / ..., which needs a group on
   `Candidate`, also for plugins), `LS_COLORS` for files, fuzzy matching.
5. **Fuzz targets** for the lexer, parser, arithmetic and pattern matcher, with a round-trip property: unparsing then
   re-parsing an AST gives the same AST.
6. **Plugins (Phase 11)**: time budgets for hooks, `plugin remove`, the example plugins of M5.

## Phase 11 — Plugin system (Stage 2)

Done: the `plugin` built-in (`load`, `list-loaded`, `list-available`, `unload`, `run`, `add`, `sync`, `update`, `check`,
and `restore` for the startup cache), directory plugins, plugin packages (below), the byte conversion, the Rhai engine
with its limits and interrupts (and without `eval`), `import` relative to the importing file and of another plugin's
modules (`@SOURCE/PLUGIN/MODULE`), the `chpwd`, `precmd`, `preexec`, `exit`, `post-rc`, `prompt-vars` and
`prompt-rewrite` hooks, completers, extension built-ins (`sh::builtin`, with `sh::read_line`), most of the `sh` module
(with `sh::capture` taking an argv, `sh::capture_sh`, `sh::which` and `sh::commands`), and the `fs` and `vcs` modules.
Still to do:

1. A plugin-agnostic `Builtin` trait, with the Rust built-ins moved onto it, so that extension and native built-ins
   go through the same code path (extension built-ins are now a separate `CommandKind::Extension`, looked up after
   functions, and `builtin`, `command`, `type` and `hash` know about them):

   ```rust
   pub trait Builtin {
       fn name(&self) -> &[u8];
       fn special(&self) -> bool { false }   // extensions can never register special built-ins
       fn run(&self, sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult;
   }
   ```

   Extension built-ins rank as **regular** built-ins in command lookup, so a shell function with the same name
   overrides them (done).
2. The rest of the `sh` module:

   | Function | Description |
   |---|---|
   | `argv0()`, `positional()` | `$0`, and `$1...` as an array |
   | `chdir(path)` | Change the directory as `cd` would, updating `PWD` and running `chpwd` hooks |
   | `parse_json(text)` | Parse JSON into Rhai maps and arrays |

3. Time budgets (checked in `on_progress`) for every hook that runs while the user waits (`precmd`, `preexec`,
   `prompt-vars`, `prompt-rewrite`, `chpwd`), not only completers. A hook over its budget is reported and skipped.
   (The `precmd`, `preexec` and `exit` hooks are done.)

4. Example plugins: a git-aware prompt, a `json` query built-in, and a command-timing `preexec`/`precmd` pair (M5).
5. Plugin packages: `plugin remove`/`gc` and `login.lsh` (below; the table, the lock, `add`, `sync`, `update` and
   `check` are done).

### Plugin packages

Done (see `DEVELOPING.md` and the plugins page of the user docs): the `[plugins]` table of `config.toml`, with
`plugins.available` (named sources: `gh`, `git` or `path`, with `branch`/`tag`/`rev` and `subdir`; `std` built in) and
`plugins.enabled` (`NAME`, `SOURCE.NAME` or `"SOURCE/NAME"`, and inline sources, all `= "*"`; collections with
sub-collections, `SOURCE/SUB/NAME`, which is also the loaded plugin's name); dependencies in a
directory plugin's `plugin.toml`, resolved recursively, and its `[options]`, `[alias]` and `[bindkey]` tables;
`plugins.lock` (pins and the resolved plugins); `plugin sync` and `plugin update`, which run git into
`$XDG_DATA_HOME/luish/plugins/`, and `plugin check`, which asks the git sources for newer commits;
`plugin load SOURCE/NAME` with dependencies; enabled plugins loaded at startup before
`rc.d`, cached with it; `post-rc.lsh` and the `post-rc` hook; `plugin add SPEC [NAME]`, which edits
`config.toml` as text; a first-run menu that writes `config.toml`. Still to do:

1. `plugin remove NAME...`, editing `config.toml` as text as `plugin add` does, and `plugin gc` for the repositories and checkouts the lock doesn't use (`sync` and `update`
   never remove them, since a running shell may still read their files).
2. `plugin list-loaded -l`: each plugin's source and commit, and the enabled plugins that aren't installed.
3. `login.lsh`: run after `login.d`, in its cache, or uncached after `~/.profile` without `login.d`.
4. `flock` on the data directory, for two `plugin sync` at once (extraction is already safe: rename into place).
5. Version requirements other than `"*"`, once plugins have versions (a `version` in `plugin.toml`, or tags).
6. More in `plugin.toml`: `description` (shown by `list-available -l`), the oldest luish a plugin needs.

**Later**: a plugin's `bin/` on `PATH` and `completions/`; per-plugin settings (`[plugins.NAME.config]`, or a settings
group of the plugin's own, such as `bashcomp.*`, with a type and a default), given to Rhai as `sh::config()`;
archives for systems without git.

## Phase 12 — Conformance and performance

Done: GNU hello and sed `configure` give the same results as under dash; 43 of 1620 Oils spec cases differ, mostly
deliberate deviations; the `PATH` cache and `posix_spawn`; scripts run as fast as under dash (see `DEVELOPING.md`).

Still to do: the startup gap (deferred), larger `configure` scripts (coreutils) and distribution scripts, the smoosh
and modernish suites, the fuzz targets, `hyperfine` in CI (non-blocking) for startup and loop regressions, including
`-c true` with and without the `plugins` feature. Optimisations only where profiling shows a need.

## Phase 13 — Replacing zsh (Stage 1, current focus)

Stage 1 asks for a daily driver "at least as good as a basic zsh setup". This phase is what luish still lacks to
replace the author's own zsh setup (`~/.zshrc` from home-manager plus zplug, and `~/.zshrc_local`), found by probing
each item against luish and from about 670 commands of typed history.

Already done: the prompt, the line-editor keys (`bindkey`, with keys by name and a `[bindkey]` table in `config.toml`, prefix search on Up/Down, `WORDCHARS`, `^O`, `Alt-.`),
autosuggestions, `auto_pushd` (with `pushd_ignore_dups` and `pushd_silent`), `CDPATH`, the directory stack, aliases
(with zsh's options, global and suffix aliases, and an `[alias]` table in `config.toml`), the conda, nvm and
home-manager setup scripts (through the `rc.d` cache), `**/`, `autocd`, menu completion, the history shared with zsh
through `~/.histfile`, history expansion (`!!`, `!$`, `^old^new`, with `setopt history.expand`), process
substitution, and grouped settings with `config.toml`.

Still to build, in this order:

1. **Completion content**, the biggest gap by volume (zsh gets git, ssh, make, man, cargo ... from `compinit` and
   zsh-completions):
   - **git**: done, in `completion` in `luish-std-plugins/` (with ranges, and the tracked files after a revision in
     `git diff REV`). Still missing: `REV:PATH`, `git config` keys, and values for most `--option=` words.
   - **Common commands**: done, as `completion` in `luish-std-plugins/` (about 230: coreutils, grep, diffutils, tar,
     make, rsync, man, ssh/scp/sftp, pkill, shells, find, sed, awk, jq, rg, compressors, systemctl, journalctl,
     loginctl, ps, mount, tmux, curl, wget, ip, gpg, openssl, cargo, rustup, go, gcc, cmake, gdb, python, editors,
     pip, uv, conda, pixi, npm, yarn, pnpm, and the package managers of systems: apt, dpkg, dnf, rpm, pacman,
     zypper, apk, brew, snap, flatpak), including **ssh/scp/rsync hosts** from `~/.ssh/config` (`Host` lines without
     wildcards, and `Include`) and `/etc/hosts`. `~/.ssh/known_hosts` is hashed on this system, so it gives nothing,
     even in zsh. Still missing: paths on remote hosts for scp and rsync, the packages that dnf, yum and zypper can
     install (their lists take seconds; zsh keeps a cache), and options for `xargs` (luish's completer skips it as a
     precommand, so no plugin sees it).
   - **A generic bridge**, so that most programs get completion without a hand-written completer: programs that
     complete themselves (Cobra, Click and nix: done, in `completion`; still to do: clap's `COMPLETE=`, `argcomplete`, Typer's),
     bash-completion scripts (as in `luish-std-plugins/bash-completion/`, but faster), and `--help` parsing for
     options only (as fish does). This needs a design note before building.
   - **Typing to narrow the menu** (zsh's `menu select interactive`): while the menu is open, printable keys filter
     the matches instead of closing the menu.
2. **Lazy function parsing for the startup cache**, below.

Hooks are the extensions' `chpwd`, `precmd`, `preexec` and `exit` (done): luish doesn't call shell functions with
zsh's hook names, nor the `*_functions` arrays (decided 2026-10-02). They wouldn't make the zsh hook snippets of
tools such as direnv, zoxide or mise work, as those use other zsh syntax too, and a hook can call a shell function
with `sh::run`. The setup's terminal title in `chpwd` becomes a small extension. If compatibility is ever wanted, a
std plugin can map the zsh names onto the hooks.

Not planned, because the history shows they aren't used or they are easy to rewrite in POSIX sh: zsh's
two-argument `cd old new`, `vared`, `zmv`, `mmv`, `zed`, `zcalc`,
`noglob`, zsh-history-substring-search, zsh-nvm, zplug, and `fpath`/`compinit`.

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

A deferred parse can fail only if the unparser doesn't round-trip, which is unit-tested; such a bug would move a
syntax error from startup to the first call. Until then, nvm can be loaded lazily, as zsh-nvm's lazy mode does: put
the default node's `bin` on `PATH` and define `nvm() { unset -f nvm; . "$NVM_DIR/nvm.sh"; nvm "$@"; }`.

**Done when:** the author uses luish as the login shell with the ported config, and the history of a week shows no
command that had to be run in zsh.

## Milestones

| Milestone | Stage | Phases | Definition of done | Status |
|---|---|---|---|---|
| M1: Runs simple scripts | 1 | 0–4 | Pipelines, redirections, compound commands, external commands | Done |
| M2: POSIX script engine | 1 | 5–9 | All expansions, built-ins, functions, traps and `set -e`; passes the differential suite | Done |
| M3: Real-world scripts | 1 | 12 (partly) | Runs autoconf `configure` scripts correctly, within ~1.5× of dash | Done |
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

- **Extensions** behind options, as `glob.star`, `glob.bare_qualifiers` and `expand.braces` (brace expansion, done)
  are, unless their syntax is an error in POSIX sh (process substitution is done, always on). With them off, POSIX
  scripts must parse and behave exactly as before and run as fast. The lexer checks the options that change parsing
  in one place (as it does `Parser::bareglobqual`); brace expansion is done when words are expanded. Indexed and associative arrays are done, always on (see `DEVELOPING.md`), with bash's
  `${!a[@]}` for the keys (and its indirection, `${!x}` and `${!prefix@}`), zsh's parameter flags (`${(k)h[@]}`,
  `${(j:,:)a[@]}`, `${(o)a[@]}` and others), and the special arrays (`pipestatus`, `path` and `dirstack`, and `match`
  and `BASH_REMATCH` for `=~`). `typeset` and `declare` are done (with `-f`, `-i`, `-l`, `-u` and zsh's `-U`), and
  extensions can read and set arrays (`sh::getarray`, `sh::getmap`). Tab completes `${a[` (indices, or keys).
  Possible follow-ups, not yet asked for: more parameter flags (`q`/`Q`, `l:n:`/`r:n:`, `#`, `e`; `P` would
  duplicate `${!x}`), flags before `#` (`${(flags)#x}`), nested substitutions (`${${x#a}%b}`), and the locale's
  collation for `(o)`. Not to be done (decided 2026-09-29): zsh's `integer` built-in and base argument
  (`typeset -i 16 x`), `typeset -F`, highlighting subscripts beyond what the highlighter already does, completing
  keys that contain `'`, and `0` rather than `''` for the holes filled in integer arrays.
- **Terminal features**: semantic prompt markers (OSC 133) and working directory reporting (OSC 7) from the REPL
  around the prompt and command output. Unicode width handling and bracketed paste belong to the line editor.
- **Scripting**: error messages with file, line and function stack (from call frames), a predictable strict mode,
  and a debugger or step-trace mode. The frames exist (`Shell::sources`, for `BASH_SOURCE`, with the file of each
  script, `.` and function call); they need the function's name and the line of the call. With those, bash's
  `FUNCNAME`, `BASH_LINENO` and `caller` and zsh's `funcstack` and `funcfiletrace` are cheap, and an option (as
  error messages are dash's by default) can print the stack with an error in a script.
- **The directory of the current file**, `~.` (decided 2026-10-02 to wait; `${BASH_SOURCE:A:h}` is done): a tilde
  prefix for `${BASH_SOURCE:A:h}`, so `. ~./lib.sh` and `cfg=~./defaults.conf` need no quoting (tilde expansion isn't
  split), in the spirit of `~+` and `~-`. dash leaves `~.` as it is (no user `.`), so this changes behaviour, though
  only for a name that can't be a user. It should take the path made absolute when the file started (joined with
  the current directory then, without a syscall), so that it still works after `cd`, unlike a relative
  `BASH_SOURCE`. Open: in `-c` and on standard input, an error or `~.` left as it is.
- **Variable provenance**: a built-in (e.g. `whereset`) that shows where each variable was set, like a more
  informative `env`:

  ```
  PATH    ~/.profile:12            prepended /home/lp/bin
          ~/.zshrc:88 → conda activate → conda.sh:412   prepended /opt/conda/bin
          inherited (sshd-session, pid 1234)
  EDITOR  ~/.profile:3
  LANG    inherited (systemd --user)
  ```

  Each `Var` records an origin: inherited, set by the shell itself, set by a built-in (`cd`, `read`, `getopts`), or
  set at a file and line (file names interned), with the call stack for assignments in functions, and "eval at
  FILE:LINE" for `eval`. Interactive shells record the full history of changes for each variable, and for list-like
  variables (`PATH`, `MANPATH`) which components each step added or removed. Scripts record nothing unless an option
  (`set -o trackvars`) is set, and the check must not slow down assignment in loops. Inherited variables can only be
  traced heuristically, by walking up the process tree (`/proc/PID/environ` is the environment at exec time, exited
  ancestors break the chain, other users' processes can't be read); a luish started by another luish could receive
  exact origins through an opt-in environment variable.
- **Interactive**: richer completion, and history with metadata (working directory, exit status, duration).
  `history.rs` owns the storage, so it can move to a structured store without changing the editor.

### Stage 3: caching of login scripts

The goal is to cache the *effects* of login scripts. With a warm cache, a new shell should start almost instantly,
and many shells started at once (a desktop login can start 40) should not each run the scripts. Built so far (see
`DEVELOPING.md`): `rc.d`, `login.d` and `_uncached.lsh`, with an entry per file keyed on `PATH`, `HOME` and the
chain, and `__luish_cache` blocks keyed on what they list (below, designed 2026-10-01). Explicit keys replace the
earlier plan of keying automatically on the inherited values the files read, since the shell can't see what the
commands they run read (`brew shellenv`, `starship init`). Every variable an entry assigns is saved, even with the
value it had.
The rest of the design is stale-while-revalidate: a shell starts from the cached state at once, reruns the scripts in
the background, and applies any difference at a later prompt, so invalidation doesn't have to be perfect.
`__luish_internal check-cache` is a first, manual form of the background run: it reruns the files in the environment
the cache was built in (which the cache records), compares the result with what the cache restores, and rebuilds the
cache or touches it, so that its modification time says when it was last validated.

#### `__luish_cache` blocks

```sh
__luish_cache env=(NVM_DIR) files=(~/.nvmrc) {
    . "$NVM_DIR/nvm.sh"
}
```

- **Syntax**: `__luish_cache`, options of the form `NAME=(...)` (each at most once, in any order, all optional), then
  a `{ ... }` body. The parser reads the options as it does `local a=(x y)` (`is_declaration`, `parse_array`), then
  the body. It is a syntax error in dash, so it is always on and costs nothing in scripts that don't use it. Unknown
  options, any other body (a subshell can't change the state) and command substitution in an option are syntax
  errors: the key is computed at every start, and `$(...)` would fork.
- **`env=(...)`**: literal variable names, checked when parsed. The key has each one's value as a shell variable when
  the block starts (so it can be one that an earlier file set), unset being distinct from empty.
- **`files=(...)`**: words expanded when the block starts (tilde, parameters, globs). The key has each path and its
  fingerprint (device, inode, size, mtime, as now); a missing file has a fingerprint of its own, so creating it
  invalidates the entry, and a directory's changes when entries are added or removed. With a glob, the list of
  matches is part of the key. This is for files that commands read (`eval "$(dircolors ~/.dircolors)"`); files read
  with `.` are tracked without it.
- **Always in the key**: the block's text, the fingerprints of the files it read with `.`, and the build of luish.
- **What is cached** is the block's effect on the state (as `state.rs` computes it now), which replaces running the
  body when the key matches. Output and other side effects happen only when the body runs. Still to do: a
  background run or `check-cache` warns about a cached block that prints.
- **Where**: built for the files of `rc.d` and `login.d` (with `_uncached.lsh`), and for `$ENV`, `luishrc` and the
  `rc.lsh` of plugins loaded there (`plugin load`, not `config.toml`'s), in a cache of their own (`startup-HOST`,
  built 2026-10-01), since neither file is cached as a whole. An entry is identified by the hash of the block's
  unparsed text and its file. Elsewhere (scripts, `-c`) the body runs as if there were no block. A block inside
  another runs as part of the outer one.
- **Later, if asked for**: `commands=(...)`, the resolved path of each command and its fingerprint, for
  `eval "$(starship init sh)"` and the like without spelling out the path; and a time to live.

#### Files in `rc.d` and `login.d`

- **A file without a top-level block** is cached whole, as if it were in `__luish_cache env=(PATH HOME) { ... }`.
  Its key also has the chain: a hash of the ids, keys and changes of the entries before it in its directory. This
  keeps per-file entries as safe as a single cache: editing `10-env.lsh` reruns it and every file after it, but not
  the files before it. (Built.)
- **A file with top-level blocks** runs every time, in its place in the order, apart from its blocks, which depend
  only on their own keys: what a block depends on outside its key is for `check-cache` to find. There is no way to
  give a whole file another key except wrapping it in a block (decided 2026-10-01; a header line would be too
  implicit).
- **`_uncached.lsh`** stays as it is, for code that is never cached: it runs every time, after the directory's other
  files.
- **`config.toml`** and the plugins it enables are an entry of their own (keyed on `config.toml`, `plugins.lock` and
  the plugins' files), first in `rc.d`'s chain. Plugins' `post-rc.lsh` are in the chain after the last file.
- **`login.d`** follows the same rules.

#### Storage and checking

- One cache file per directory, and one (`startup-HOST`) for the blocks of `$ENV`, `luishrc` and the plugins they
  load (built). Each holds the entries of its files and blocks, with the 4 most recent keys of each, so that login
  and other shells, or shells started with and without `conda activate`, don't evict each other's entries. Warm
  path: one read per cache file, and a stat per startup file and per `files=` path.
- The cache records the environment of the last shell that built an entry, and `check-cache` reruns the startup as
  that shell would, rebuilding every entry it reaches, and reports which differ (built, including `startup`).
  Entries built in other environments are kept, but not checked: recording each build's environment (stored once
  per build, not per entry) would check them too. Still to do: while checking (so at no cost otherwise), record the
  inherited variables a block expands without listing them, and warn: `rc.d/20-nvm.lsh:2: the block reads
  SSH_CONNECTION, which is not in its key`.

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
