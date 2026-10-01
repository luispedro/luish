# Built-in commands compared

This page lists every built-in command of dash, bash, zsh and luish, and for those luish lacks, what to use instead.
It complements [](index.md); [](../builtins.md) documents luish's own built-ins in full.

The lists are those of dash 0.5.12 (Debian and Ubuntu's), bash 5.2 (`compgen -b`), zsh 5.9 (`${(k)builtins}`
with no startup files, which includes the built-ins of modules zsh loads on demand, such as `zstyle` and the
completion system's) and luish 0.3.0. zsh's other modules, loaded with `zmodload` (such as `zsh/files`, `zsh/datetime`
or `zsh/system`), add more, and bash can load more with `enable -f`; those are not listed.

| | dash | bash | zsh | luish |
|---|--:|--:|--:|--:|
| Built-ins | 38 | 61 | 103 | 52, and 4 more in interactive shells |

In the tables, ✓ marks a built-in, – its absence, and *i* a built-in that luish has only in interactive shells (and
their subshells), so that scripts find the same commands as under dash; there, `__luish_internal NAME` runs it.

## POSIX special built-ins

All four shells have these. As POSIX requires, an error in a special built-in exits a non-interactive shell, and
assignments before one stay set; luish follows dash in both.

| Built-in | dash | bash | zsh | luish | luish compared with bash and zsh |
|---|:-:|:-:|:-:|:-:|---|
| `:` | ✓ | ✓ | ✓ | ✓ | |
| `.` | ✓ | ✓ | ✓ | ✓ | Further arguments are the positional parameters while the file runs, as in bash and zsh |
| `break` | ✓ | ✓ | ✓ | ✓ | |
| `continue` | ✓ | ✓ | ✓ | ✓ | |
| `eval` | ✓ | ✓ | ✓ | ✓ | |
| `exec` | ✓ | ✓ | ✓ | ✓ | No `-a`, `-c` or `-l` |
| `exit` | ✓ | ✓ | ✓ | ✓ | |
| `export` | ✓ | ✓ | ✓ | ✓ | Takes arrays (`export a=(x y)` sets and exports `a`); no `-n` or `-f` (bash) |
| `readonly` | ✓ | ✓ | ✓ | ✓ | Takes arrays; no `-f` (bash) |
| `return` | ✓ | ✓ | ✓ | ✓ | |
| `set` | ✓ | ✓ | ✓ | ✓ | POSIX's options, with `-o pipefail`; luish's own and zsh-named options go through `setopt` (or `-o NAME`) |
| `shift` | ✓ | ✓ | ✓ | ✓ | |
| `times` | ✓ | ✓ | ✓ | ✓ | |
| `trap` | ✓ | ✓ | ✓ | ✓ | Signals and `EXIT`; no `ERR`, `DEBUG` or `RETURN` (bash), `ZERR` or `TRAP*` functions (zsh), and no `-p` or `-l` |
| `unset` | ✓ | ✓ | ✓ | ✓ | `-f` and `-v`; elements of arrays (`unset 'a[1]'`) |

## POSIX regular built-ins and utilities

The commands that POSIX requires to be built in (`alias`, `bg`, `cd`, `command`, `fc`, `fg`, `getopts`, `hash`,
`jobs`, `kill`, `read`, `type`, `ulimit`, `umask`, `unalias`, `wait`), and the utilities that shells build in for
speed.

| Built-in | dash | bash | zsh | luish | luish compared with bash and zsh |
|---|:-:|:-:|:-:|:-:|---|
| `[`, `test` | ✓ | ✓ | ✓ | ✓ | POSIX's operators, as dash; `-v NAME` only in `[[ ... ]]` |
| `alias` | ✓ | ✓ | ✓ | ✓ | zsh's options (`-g` and `-s` for global and suffix aliases, `-r`, `-m`, `-L`, `+`); no `-p` (bash) |
| `bg` | ✓ | ✓ | ✓ | ✓ | |
| `cd` | ✓ | ✓ | ✓ | ✓ | `-L`, `-P`, `-e`, `cd -`, `CDPATH`, and zsh's `auto_pushd`; no `cd OLD NEW` (zsh) |
| `command` | ✓ | ✓ | ✓ | ✓ | `-p`, `-v`, `-V` |
| `echo` | ✓ | ✓ | ✓ | ✓ | As dash's: escapes such as `\t` are always interpreted, and `-n` is the only option (bash needs `-e`; zsh interprets them unless `BSD_ECHO` is set) |
| `false`, `true` | ✓ | ✓ | ✓ | ✓ | |
| `fc` | – [^fc] | ✓ | ✓ | ✓ | `-e`, `-l`, `-n`, `-r`, `-s`; no times (zsh's `-d`, `-i`) or zsh's other options |
| `fg` | ✓ | ✓ | ✓ | ✓ | |
| `getopts` | ✓ | ✓ | ✓ | ✓ | |
| `hash` | ✓ | ✓ | ✓ | ✓ | `-r`; no `-d`, `-p`, `-t`, `-l`. Rarely needed: luish clears the cache itself when a `PATH` directory changes |
| `jobs` | ✓ | ✓ | ✓ | ✓ | `-l`, `-p`; no `-r`, `-s`, `-n`, `-x` |
| `kill` | ✓ | ✓ | ✓ | ✓ | `-s`, `-SIGNAL`, `-l`, and jobs (`%1`) |
| `printf` | ✓ | ✓ | ✓ | ✓ | `%b`; no `-v` or `%q` |
| `pwd` | ✓ | ✓ | ✓ | ✓ | `-L`, `-P` |
| `read` | ✓ | ✓ | ✓ | ✓ | `-r`, `-p`, and arrays with `-a` (bash) or `-A` (zsh); no `-t`, `-n`, `-s`, `-d`, `-u`, `-e` or zsh's `-k`, `-q` |
| `type` | ✓ | ✓ | ✓ | ✓ | No options (bash's `-a`, `-t`, `-p`; zsh's `-a`, `-w` ...); names the plugin of a built-in that a plugin added |
| `ulimit` | ✓ | ✓ | ✓ | ✓ | `-H`, `-S`, `-a` and dash's resource letters |
| `umask` | ✓ | ✓ | ✓ | ✓ | `-S` |
| `unalias` | ✓ | ✓ | ✓ | ✓ | `-a`, and zsh's `-m`, `-s` |
| `wait` | ✓ | ✓ | ✓ | ✓ | Jobs and process ids; no `-n`, `-f` or `-p` (bash) |

[^fc]: Debian's dash is built without a line editor, so it has no `fc`. Upstream dash built with libedit has one.

## Common extensions

Built-ins outside POSIX that luish has, mostly as zsh has them.

| Built-in | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `bindkey` | – | – | ✓ | *i* | zsh's, for the emacs keymap and the widgets luish has; keys can also be named (`Up`, `Ctrl-Right`). No other keymaps (`-M`) and no user-defined widgets |
| `builtin` | – | ✓ | ✓ | ✓ | |
| `chdir` | ✓ | – | ✓ | ✓ | The same as `cd` |
| `declare`, `typeset` | – | ✓ | ✓ | ✓ | `-a`, `-A`, `-f`, `-g`, `-i`, `-l`, `-p`, `-r`, `-u`, `-U`, `-x`, and `+f`; no namerefs (bash's `-n`), floating point (zsh's `-E`, `-F`), padding (zsh's `-L`, `-R`, `-Z`), `-t` or tied variables (zsh's `-T`) |
| `dirs`, `pushd`, `popd` | – | ✓ | ✓ | ✓ | As in zsh: `-q`, `-L`, `-P`, `+n`, `-n`; `dirs -l`, `-p`, `-v`, `-c` |
| `help` | – | ✓ | – | *i* | Help for luish's built-ins, from the pages in [](../builtins.md) |
| `let` | – | ✓ | ✓ | ✓ | As in zsh; integers only, and without `**` (as dash's arithmetic) |
| `local` | ✓ | ✓ | ✓ | ✓ | Special, as in dash. Takes `typeset`'s options and arrays (`local a=(x y)`). Without a value, keeps the outer value, as in dash (in bash and zsh it starts unset or empty) |
| `print` | – | – | ✓ | *i* | zsh's, with its options except `-p` (no coprocesses) and `-S` |
| `setopt`, `unsetopt` | – | – | ✓ | ✓ | zsh's names for the options luish has, and luish's grouped names (`history.share`) and `NAME=VALUE` settings |
| `source` | – | ✓ | ✓ | ✓ | As in zsh: a name without `/` is looked for in the current directory first |

## Built-ins that luish lacks

### In both bash and zsh

| Built-in | What it does | In luish |
|---|---|---|
| `disown` | Remove a job from the job table | Not yet. `nohup` or `setsid` for new commands |
| `enable` (zsh's also `disable`) | Turn built-ins off or on | `command NAME` or a full path runs the program instead |
| `history` | List the history | `fc -l` |
| `logout` | Exit a login shell | `exit` |
| `suspend` | Stop the shell | `kill -STOP $$` |

### bash only

| Built-in | What it does | In luish |
|---|---|---|
| `bind` | readline key bindings | `bindkey`, or `[bindkey]` in `config.toml` |
| `caller` | The call stack, for debugging | None |
| `compgen`, `complete`, `compopt` | Programmable completion | Completers in Rhai, from [plugins](../plugins.md); `std.bash-completion` runs bash's completion scripts |
| `mapfile`, `readarray` | Read lines into an array | A loop: `while IFS= read -r l; do a+=("$l"); done` |
| `shopt` | bash's own options | `setopt`, for the options luish has (luish suggests the name for some, such as `globstar`) |

### zsh only

| Built-in | What it does | In luish |
|---|---|---|
| `-` | Run a command as a login shell would (`argv[0]` with `-`) | None |
| `autoload` | Define a function to be loaded from `fpath` on first use | None: `.` the file in a startup file, which the [startup cache](../usage.md#cached-startup-files) makes cheap |
| `bye` | Exit | `exit` |
| `compadd`, `comparguments`, `compcall`, `compdescribe`, `compfiles`, `compgroups`, `compquote`, `compset`, `comptags`, `comptry`, `compvalues` | The completion system (compsys) | Completers in Rhai, from [plugins](../plugins.md) |
| `compctl` | The old completion system | As above |
| `echotc`, `echoti` | Terminal capabilities | `tput` |
| `emulate` | Emulate sh, ksh or csh | None: luish is always an `sh` |
| `float`, `integer` | Floating-point and integer variables | `typeset -i` for integers; no floating point |
| `functions` | List or define functions | `typeset -f` |
| `getln`, `pushln` | The buffer stack | None |
| `limit`, `unlimit` | csh-style resource limits | `ulimit` |
| `log` | Report logins and logouts (`watch`) | None |
| `noglob` | Run a command without globbing | `set -f`, or quoting |
| `private` | Variables not seen by called functions | `local` (seen by called functions) |
| `r` | Re-run a command | `fc -s` (`alias r='fc -s'`) |
| `rehash` | Clear the command cache | Rarely needed (automatic); `hash -r` |
| `sched` | Run commands at a given time | None |
| `ttyctl` | Freeze the terminal's settings | None |
| `unfunction`, `unhash` | Remove functions, aliases or cached commands | `unset -f`, `unalias`, `hash -r` |
| `vared` | Edit a variable in the line editor | None |
| `whence`, `where`, `which` | Describe or find commands | `type`, `command -v`, `command -V` |
| `zcompile` | Compile scripts | None needed for startup files: the [startup cache](../usage.md#cached-startup-files) saves their effect |
| `zformat`, `zparseopts`, `zregexparse` | Formatting and parsing helpers | `printf`, `getopts`, `[[ =~ ]]` |
| `zle` | Line editor widgets | `bindkey` binds keys to luish's widgets; no user-defined widgets |
| `zmodload` | Load modules | [Plugins](../plugins.md), with extensions in Rhai that can add commands |
| `zstyle` | Style settings, mostly for completion | Settings in `config.toml` and `setopt NAME=VALUE` |

## luish's own

| Built-in | What it does |
|---|---|
| `__luish_internal` | Subcommands that are not needed often: `check-cache` (check the startup cache), `savestate` (print commands that restore the shell's state), `complete` (show what Tab would offer), `print-git-rev`, and the interactive-only built-ins, which work there in scripts too |
| `plugin` (*i*) | Load, list, add, fetch and update [plugins](../plugins.md) |

Plugins can add further built-ins, written in Rhai (see [](../extensions.md)).

## Reserved words

Some commands that look like built-ins are reserved words, parsed with the command.

| Word | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `!`, `{ }`, `case`, `for`, `if`, `until`, `while` ... | ✓ | ✓ | ✓ | ✓ | POSIX |
| `[[ ]]` | – | ✓ | ✓ | ✓ | |
| `function` | – | ✓ | ✓ | ✓ | |
| `time` | – | ✓ | ✓ | – | Runs the `time` program instead, which can't time pipelines or shell functions |
| `select` | – | ✓ | ✓ | – | |
| `coproc` | – | ✓ | ✓ | – | |
| `repeat`, `foreach`, `nocorrect` | – | – | ✓ | – | |
| `declare`, `export`, `local`, `readonly`, `typeset`, `float`, `integer` | – | – | ✓ | – | In zsh, these are reserved words so that `local a=(x y)` parses. luish parses such array assignments for its own built-ins, which remain built-ins |
