# What's new

## Version 0.6.0 (10 October 2026)

This release brings remote shells whose line editor runs on your machine,
zsh's `vared`, and sources that plugins can make available.

**Remote shells that feel local.** `luish --ssh HOST` logs in to HOST with
ssh and runs your commands there, but edits command lines on your machine:
typing, moving in the line, the history, autosuggestions and the completion
menu don't wait for the network, while Tab completion, the prompt and your
startup files still come from HOST (see [](ssh.md)).

- luish needn't be installed on HOST: `luish --ssh` copies itself there the
  first time, in the same connection, and keeps one copy per version
  (`-o ssh.no_auto_copy` never copies, `--copy-luish` always does, and
  `--luish-path=PROGRAM` runs a luish already there).
- Commands run on a terminal of their own on HOST, so full-screen programs,
  job control, `^C`, `^Z` and window size changes work as over ssh; keys
  typed while a command runs start the next line.
- ssh's escapes `~.`, `~^Z`, `~?` and `~~` work as in ssh
  (`-o ssh.escape_char` changes `~`).
- `-e COMMAND` (or `--rsh=COMMAND`, `--ssh-command=COMMAND`) uses COMMAND
  instead of `ssh -T`, as in rsync; `luish --remote COMMAND...` works over
  any COMMAND that starts `luish --serve` at the other end.
- Your terminal's variables (`TERM`, `COLORTERM`, `TERM_PROGRAM`) and your
  locale (where HOST has none) reach HOST.
- `std.completion` completes `luish --ssh` (ssh's options, luish's own and
  hosts) and `luish --remote COMMAND`.

**`vared`**, as in zsh, edits the value of a variable with the line editor:
`vared PATH`, or `vared -p 'Message: ' -c msg` to ask for one (see
`help vared`).

**Plugins.** A plugin's `plugin.toml` can make sources available in its
`[available]` table, as `config.toml`'s `plugins.available` does, so that a
personal plugin can bring the sources of the plugins you load now and then
to every machine (see
[](plugins.md#making-sources-available-available)).

**Other improvements.** A bad glob qualifier names the word it is in, as it
may come from an expansion (`echo $PS1` with a prompt that ends in
`%(...)`).

## Version 0.5.0 (5 October 2026)

This release brings more of zsh's and bash's language, a way to find where
variables were set, a menu of the jobs, and options for plugins.

**Where variables were set.** With `setopt vars.trace` (or
`luish -o vars.trace`, to trace the startup files too), the new `where`
built-in shows the file and line that last set a variable, and the function
that ran. `setopt vars.trace_history` keeps the last changes of each
variable, which `where -a` lists, numbered and in colour (see
[](usage.md#tracing-variables)).

**Jobs.**

- `jobs -i` shows a menu of the jobs, to bring one to the foreground,
  continue, stop or kill it (`K`, then a key for the signal: `e` sends
  `TERM`, then `KILL` if the job is still there 5 s later).
- `disown`, as in zsh, with bash's `-a`, `-r` and `-h`, and jobs named by a
  process id (`disown $!`).
- `cmd &|` and `cmd &!` run `cmd` in the background and disown it, as in zsh.

**The language.**

- `a |& b` pipes standard error too, as in zsh and bash.
- `case` arms can end with `;&` (fall through to the next arm), or with `;|`
  or `;;&` (go on trying the next patterns), as in zsh and bash.
- Arithmetic has `**` (and `**=`), `++` and `--` (prefix and postfix), and
  the comma operator, as in zsh and bash.

**Plugins.**

- Plugin options: a plugin declares them in its `plugin.toml`'s
  `[plugin-options]`, and they are given in `plugins.enabled`, in
  dependencies, or as `plugin load NAME OPTION=VALUE`; extensions see them
  as `sh::plugin_options()` and shell code as `$LUISH_PLUGIN_OPTIONS` (see
  [](plugins.md#plugin-options-plugin-options)).
- `luish-version` in `plugin.toml` names the oldest luish a plugin needs; an
  older luish doesn't load the plugin, and says why (see
  [](plugins.md#the-oldest-luish-a-plugin-needs-luish-version)).
- `sh::capture_cached` is `sh::capture` with its output kept in memory for a
  while; the standard completers that read a program's `-h` use it, so
  pressing Tab again doesn't rerun the program.
- The `__luish_cache` blocks of plugins that `config.toml` enables (or that
  an `rc.d` file loads) are cached with keys of their own.

**Other improvements.** `help` is shown in colour on a terminal, with the
colour scheme in use, and large scripts parse faster, with less memory.

**Fixes.**

- `local -` restores the options when the function returns, as in dash.
- `test FILE -nt MISSING` (and `MISSING -ot FILE`) is true when `FILE`
  exists, as in POSIX and dash; `[[ ... ]]` too, as in bash.
- An empty table in `plugins.enabled` (`std.completion = { }`) enables the
  plugin, as `"*"` does, instead of nothing.

## Version 0.4.0 (4 October 2026)

This release is about the terminal: colour schemes and styles for the line
editor, richer highlighting, integration with the terminal emulator, and error
messages that show where they came from.

**Colours and highlighting.**

- Styles and colour schemes: highlighting, the completion menu and
  suggestions take their colours from named styles, set with the `style`
  built-in or in `[style]` and `[colorscheme.NAME]` tables in `config.toml` or
  a plugin's `plugin.toml`. A scheme can be a dark/light pair, chosen by the
  terminal's background colour (detected, or from `$LUISH_BACKGROUND` or
  `$COLORFGBG`), and can set the terminal's own colours (see
  [](usage.md#styles) and [](colour-schemes.md)).
- Highlighting tells kinds of command apart (built-in, function, alias,
  external, precommand), marks options, escapes, special parameters, tildes,
  braces and globs, shows exported, array and read-only variables, and marks
  syntax errors. `setopt highlight.paths` marks words that name files, and
  `setopt editor.no_highlight` turns highlighting off.
- Prompts can use the same styles, with `%[style:NAME]` and `%[style_off]`,
  and more colour names in `%F{...}` (`bright-red` and the like). Styles can
  be curly, double, dotted or dashed underlines, in a colour of their own.

**The terminal.** Prompt marks (OSC 133) and the current directory (OSC 7),
so that terminals can jump between prompts and open new tabs in the same
directory (`setopt terminal.no_integration` turns them off; see
[](usage.md#terminal-integration)). `clipcopy` copies to the clipboard through
the terminal, the new `std/notify` plugin sends a desktop notification when a
long command ends, file names in interactive error messages are links, and in
vi mode the cursor's shape shows the input mode.

**Error messages** name the file of the code that failed (not `$0`), and show
the failing line and the call stack (see [](usage.md#error-messages)).

**The language.**

- Here-strings, `cmd <<< word`, as in bash and zsh.
- Brace expansion, `a{b,c}` and `{1..10}`, with `setopt expand.braces`.
- zsh's modifiers in parameter expansion: `${x:h}`, `${x:t}`, `${x:r}`,
  `${x:e}`, `${x:a}`, `${x:A}`, `${x:u}`, `${x:l}`, and `:a` and `:A` in glob
  qualifiers and history expansion.
- Array slices, `${a[i..j]}`, as in Python.
- bash's `BASH_SOURCE` (so `${BASH_SOURCE:A:h}` is the script's directory),
  `FUNCNAME`, `BASH_LINENO` and `caller`.

**Configuration.** `config.toml` can set variables (`[env]`,
`[env.interactive]` and `[vars]`) and add directories to `PATH` (`[path]`; see
[](usage.md#settings-in-configtoml)). `install.sh` offers to write the
recommended configuration, with [luish-extra](plugins.md#more-completion-luish-extra)'s
completion for about 270 more commands.

**Plugins.**

- Plugins of a collection have full names, such as `std/completion`, so that
  short names don't clash, and collections can have sub-collections
  (`SOURCE/SUB/NAME`, or `SOURCE.SUB.NAME` in TOML).
- `plugin` uses colours and says more: `load`, `unload` and `add` say what
  they did, and `sync`, `update` and `check` list each source, with its
  plugins and a link to compare versions.
- `plugin run` runs Rhai code once, from a file or with `-c`, and `plugin load
  -c CODE NAME` loads a plugin without a file.
- Extensions can have `precmd`, `preexec` and `exit` hooks (see
  [](extensions.md#hooks)).
- `std/completion` completes luish's own built-ins (options, styles and colour
  schemes).

**Incompatible changes.** `$LUISH_HIGHLIGHT` is gone: use styles (`[style]` in
`config.toml`, or the `style` built-in) instead. Plugins of a collection are
loaded under their full name (`std/completion`, not `completion`), which
`plugin list-loaded` shows and `plugin unload` takes.

**Fixes.** `typeset -f` and the saved state now print back `${x/}`,
`` ${`cmd`} ``, a `$` before backquotes and a few other constructs correctly.
Some inputs that made luish slow or crash are now errors: an array index of
64 Mi or more, `!{}` and history expansions of more than 1 MiB, `%NG` with a
huge N, and deeply nested `${a[${a[...}}` or `$((` that isn't arithmetic.

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


