# What's new

## Version 0.3.0 (1 October 2026)

This release is about daily use: more of zsh's interactive features, completion
for many more commands, a faster startup cache that you can tune, and plugins
that can add commands.

**Interactive use.**

- History expansion: `!!`, `!$`, `!-2`, `!str`, `^old^new` and the rest, with
  `setopt history.expand` (off by default, since `!` isn't special in POSIX
  sh), and `history.verify` to edit the line before running it (see
  [](usage.md#history-expansion)).
- The right prompt, as in zsh: `RPROMPT` (or `RPS1`) and `RPROMPT2`, with
  `ZLE_RPROMPT_INDENT` and `setopt prompt.transient_rprompt` (see
  [](usage.md#the-right-prompt)).
- zsh's `print`, with `-P` for prompt expansion.
- On the first interactive run, a menu offers to write a `config.toml` with the
  recommended settings (see [](getting-started.md#the-first-run)).

**Completion.** `std.completion` now completes about 230 commands, up from 70:
shells, `find`, `sed`, `jq`, `rg`, compressors, `systemctl`, `journalctl`,
`tmux`, `curl`, `ip`, `gpg`, `cargo`, `rustup`, `go`, `gcc`, `cmake`, `python`,
`pip`, `uv`, `conda`, `pixi`, `npm`, `apt`, `dnf`, `pacman`, `brew`, `flatpak`
and more. Programs built with Cobra (`gh`, `docker`, `kubectl` ...) or Click
(`black`, `flask` ...), and nix, are asked for their own completions, and
`complete-cobra` and `complete-click` register more of them. git completion is
now part of `std.completion` (the separate `std.git-completion` is gone).

**The language.** Process substitution, `<(cmd)` and `>(cmd)`, as in bash and
zsh (see [](usage.md#shell-language-extensions)). `LUISH_VERSION`,
`LUISH_PATCHLEVEL`, `MACHTYPE`, `HOSTTYPE` and `OSTYPE` tell a script which
shell, build and system run it.

**Startup cache.** Each file of `rc.d` and `login.d` has an entry of its own,
so editing one file reruns only it and the files after it, and
`PATH=$HOME/bin:$PATH` in a startup file adds to the `PATH` of each shell.
`__luish_cache` blocks cache part of a file, keyed on the variables and files
they list, also in `$ENV` and `luishrc` (see
[](usage.md#caching-part-of-a-file)):

```sh
__luish_cache env=(NVM_DIR) files=("$NVM_DIR/alias/default") {
    . "$NVM_DIR/nvm.sh"
}
```

`check-cache` reports differences by file and block.

**Plugins.**

- `plugin add` adds a plugin (a GitHub repository, a git or file URL, or a
  path) to `config.toml` and fetches it.
- Extensions can add commands: `sh::builtin(name, fn)` registers a built-in
  written in Rhai, which `sh::read_line()` lets read its input, and `type`
  names the plugin that added it (see [](extensions.md)).
- New functions for extensions: `sh::which`, `sh::commands`, `sh::matches`,
  `sh::expand_prompt` and `fs::realpath`. `sh::capture` now takes the program
  and its arguments as an array (`sh::capture_sh` runs shell code, as
  `sh::capture` did).
- `import` is relative to the importing file, and `import
  "@SOURCE/PLUGIN/MODULE"` imports another plugin's module, such as std's
  completion engine.
- Library plugins (`library = true` in `plugin.toml`), hidden by `plugin
  list-available` unless given `-a`.
- Extensions can no longer use Rhai's `eval`.

**Incompatible changes.** `sh::capture` takes an array instead of shell code
(use `sh::capture_sh` for the old behaviour); `std.git-completion` is gone
(enable `std.completion`); `install.sh` no longer reads `LUISH_VERSION` (use
`--version`).

**Fixes.** git completion of ranges (`HEAD^..`) and of files after a
revision; `umount` completion of mount points with a newline or backslash in
them; `*` in an ssh `Include` no longer matches files that start with `.`.

## Version 0.2.0 (29 September 2026)

This release brings bash's and zsh's scripting extensions to luish. Arrays,
associative arrays, `[[ ... ]]`, `typeset` and the rest work in scripts and in
the interactive shell, and scripts that don't use them run as fast as before
(see [](performance.md)).

**Arrays.** Indexed and associative arrays, with the syntax and behaviour of
zsh's `sh` emulation, and bash's where that is silent (see
[](usage.md#arrays)):

```sh
files=(*.txt "my notes")               # indexed, counting from 0
files+=(extra)
echo "${files[-1]}" "${#files[@]}"
typeset -A size=([small]=1 [big]=10)   # associative
for k in "${!size[@]}"; do
    echo "$k=${size[$k]}"
done
```

- `read -A` (zsh) and `read -a` (bash) read a line into an array; `local`, `export` and `readonly` take arrays.
- bash's `${!a[@]}` (the indices or keys) and `${!x}` (indirection, also `${!prefix@}`).
- zsh's parameter flags: `${(j:,:)a[@]}` (join), `${(s:/:)PWD}` (split), `${(o)a[@]}` (sort), `${(k)h[@]}` (keys),
  `u`, `O`, `i`, `n`, `L`, `U`, `C` and more.
- `typeset` and `declare`, with `-a -A -i -l -u -U -f -g -p -r -x`. `typeset -U path` keeps `PATH` free of repeated
  directories, and `typeset -f` prints functions.
- Tab after `${a[` completes the indices or keys.
- Plugins can read and set arrays: `sh::getarray`, `sh::getmap` and
  `sh::setvar` with an array or a map.

**More of the language.**

- `[[ ... ]]` conditions, with patterns, `=~` regular expressions (setting `match`, `mbegin`, `mend` and
  `BASH_REMATCH`), `&&`, `||` and `-v`.
- `function NAME { ...; }`, `NAME+=value`, `${x:offset:length}` and `${x/pattern/replacement}`.
- `let`, and `builtin` (as in zsh and bash).
- `set -o pipefail`, from POSIX 2024.
- zsh's special variables: `RANDOM`, `SECONDS`, `EPOCHSECONDS`, `EPOCHREALTIME`, `UID`, `EUID`, `GID`, `EGID`,
  `HISTCMD`, and the arrays `pipestatus` (`PIPESTATUS` in bash), `path` (tied
  to `PATH`) and `dirstack` (tied to the directory stack). `SHLVL` is
  maintained.

**Interactive use.**

- Tab expands globs, variables and command substitutions in the word, as zsh
  does: `ls *.md` Tab becomes the list of files.
- The syntax highlighter colours unset variables differently (`unset` in
  `$LUISH_HIGHLIGHT`).
- `shopt` suggests the `setopt` that does the same.

**Startup cache.** `__luish_internal check-cache` reruns the startup files and
compares the result with what the cache restores, and rebuilds it if they
differ. It can run from `cron` (see [](usage.md#checking-the-caches)).

**Fixes.** Crashes on Tab after a stray `)` or after a word like `--opt=value`,
and on a plugin source with an empty path. luish no longer prints git's error
for a locked plugin commit that isn't fetched yet.

**Known problem.** The startup cache records what your files changed relative
to the environment of the shell that built it. If `conda` or `nvm` fail in
new terminals, remove the caches (`rm ~/.cache/luish/rc-*
~/.cache/luish/login-*`) and start luish once from a terminal that isn't
running another shell with the same setup. See [Cached startup
files](usage.md#cached-startup-files). A fix is planned.

**Upgrading.** The startup caches rebuild themselves the first time. Scripts
that follow POSIX behave as before, with few exceptions.

## Version 0.1.0 (27 September 2026)

The first release.

**Scripts.** The POSIX shell language and its built-ins, plus `local`, behave
as in dash. GNU `configure` scripts give the same results as under dash.

Scripts run as fast as under dash (see [](performance.md)).

**Startups are cached**: a new terminal is started in &lt; 20 ms even when
using frameworks like `conda` and `nvm` that set many options.

**Interactive use.**

- Line editing with zsh's emacs keys (or vi keys), syntax highlighting, and
  autosuggestions from the history (`editor.autosuggest`).
- History in zsh's file format, which luish and zsh can share, optionally
  between running shells (`history.share`), with `fc` and search as you type.
- Tab completion with an interactive menu and a standard library of completions
  for &gt; 70 common commands and support for falling back on bash-completion
  scripts.
- zsh's prompt sequences (`%~`, `%?`, `%F{...}`, conditions, ...), improved
  with better variable names.
- Job control (`jobs`, `fg`, `bg`, Ctrl-Z), and zsh's directory stack (`pushd`, `popd`, `dirs`).
- zsh's recursive globbing (`**/`) and glob qualifiers (`*(.om[0])`).
- `help` for every [builtin](builtins.md)

**Configuration.** Settings go in `~/.config/luish/config.toml` and
`~/.config/luish/rc.d`.

**Plugins.** Plugins are directories of shell files, with optional code in
[Rhai](https://rhai.rs) for hooks, prompt variables and Tab completion. The can
be automatically installed from GitHub and can be pinned to specific commits.
This is built-in to luish rather than built with a separate plugin manager.


