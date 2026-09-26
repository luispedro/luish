# Usage

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`. In the shell, `set` takes the
same options, and `setopt` and `unsetopt` set them by name as in zsh, together with luish's own options (such as
`promptpercent`), which `set` doesn't show so that it stays as in dash.

## Getting help

In an interactive shell, `help` lists the built-in commands, and `help NAME` shows the help for one of them (the
same text as in [](builtins.md)). `help` is not a built-in in scripts, so that they find the same commands as in
other shells; there, `__luish_internal help` does the same.

## Prompts

`PS1` is the prompt, `PS2` the prompt for the continuation lines of a command, and `PS4` the prefix of the lines
that `set -x` prints. As POSIX requires, they go through parameter expansion, so `PS1='$PWD\$ '` shows the current
directory.

With the `promptpercent` option (`setopt prompt_percent`), they then also expand `%` sequences, as in zsh:

```sh
setopt prompt_percent
PS1='%F{blue}%~%f %(?..%F{red}[%?]%f )%# '
```

This shows the current directory (with `~` for `$HOME`) in blue, then the exit status of the last command in red if
it failed, then `#` for root and `%` for other users. Parameter expansion comes first, so a `%` in the value of a
variable is expanded too; write `%%` for a literal `%`. The sequences are those of zsh:

| Sequence | Expands to |
|---|---|
| `%%`, `%)` | `%`, `)` |
| `%~`, `%d` or `%/` | The current directory, with or without `~` for `$HOME`. With a number, `%N~` gives only its last `N` components, and `%-N~` its first `N` |
| `%c` or `%.`, `%C` | The last component of the current directory, with or without `~` (`%Nc` for more) |
| `%n`, `%m`, `%M` | The user name, the host name up to the first `.` (`%Nm`: `N` components), the full host name |
| `%#` | `#` for root, `%` otherwise |
| `%?` | The exit status of the last command |
| `%h` or `%!` | The number of the next history event |
| `%j` | The number of jobs |
| `%L`, `%i` | `$SHLVL`, the line number (for `PS4`) |
| `%l`, `%y` | The terminal, without `/dev/` (and, for `%l`, without `tty`) |
| `%D`, `%T`, `%*`, `%t` or `%@`, `%w`, `%W` | The date as `yy-mm-dd`, the time as `HH:MM` or `HH:MM:SS`, or in 12-hour format, the weekday and day, the date as `mm/dd/yy` |
| `%D{format}` | The time in a `strftime` format (and zsh's `%f`, `%K` and `%L`, the day and hours without padding) |
| `%B` `%b`, `%U` `%u`, `%S` `%s` | Start and stop bold, underline and standout (reverse video) |
| `%F{colour}` `%f`, `%K{colour}` `%k` | Start and stop a foreground and a background colour: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, a number from 0 to 255, or `#rrggbb`. `%NF` is `%F{N}` |
| `%E` | Clear to the end of the line |
| `%{...%}` | Text written as it is, taking no room on the screen: for other escape sequences, such as a terminal title |
| `%NG` | Within `%{...%}`: the escape sequence takes `N` columns |
| `%(x.yes.no)` | `yes` if the condition `x` holds, otherwise `no` (any character can replace the `.`s). The conditions take a number `N`, as in `%(N?.yes.no)` or `%N(?.yes.no)`: `?` the exit status is `N` (0 by default), `#` the user id is `N` (0: root), `!` the shell runs as root, `g` the group id is `N`, `j` there are at least `N` jobs, `L` `$SHLVL` is at least `N`, `/` or `C` the current directory has at least `N` components, `~`, `.` or `c` the same, with `~` for `$HOME` counting as one; `T`, `t`, `d`, `D` and `w`: the hour, minute, day of the month, month (from 0 for January) or day of the week (from 0 for Sunday) is `N` |
| `%N<text<`, `%N>text>` | Shorten what follows (up to the end of the enclosing `%(...)`, or to the next `%<<`) to `N` characters, replacing what is cut on the left or the right by `text` |

Other sequences expand to nothing. zsh's `%_`, `%e`, `%I`, `%N`, `%x`, `%v`, `%[...]`, and conditions and truncation
widths relative to the terminal's width, aren't supported.

## Tab completion

In an interactive shell, Tab completes the word under the cursor: a command name (a built-in, function, alias or
program in `PATH`) at the start of a command, a variable name after `$` or `${`, a user's home directory after `~`,
and a filename elsewhere, also after the `=` or `:` of an assignment and the `=` of a `--option=`. After commands
that run another command, such as `sudo`, `env`, `nohup`, `time` and `xargs`, the command they run (after their
options) completes as a command name. Some commands complete their arguments differently:

- `cd`, `pushd` and `rmdir` complete directories;
- `export`, `local`, `readonly`, `unset`, `read` (except the prompt after `-p`), `getopts` (after the option
  string) and `for` (then `in`) complete variable names (`unset -f` completes function names);
- `alias` and `unalias` complete aliases;
- `type`, `hash` and `which` complete command names, and `help` completes built-ins;
- `fg`, `bg`, `jobs`, `wait` and `kill` complete job specs such as `%1`, listed with their commands (after `%` and
  a letter, they complete the command names instead, such as `%vim`);
- `kill -` and `kill -s` complete signal names, as do the arguments of `trap` after its action;
- `plugin load` completes the plugins in the plugin directory, and `plugin unload` the loaded ones.

Plugins can provide completion for other commands (see [Plugins](plugins.md)). Aliases are followed: if `g` is an
alias for `git`, then `g ` completes as `git ` does.

The first Tab completes as much as is common to all the matches, and a second one lists them. What is added is quoted
as needed: a file called `my file` is completed as `my\ file`, or as `'my file'` after a `'`. A directory is completed
with a `/`, so Tab can go on into it; any other single match gets a space (and the closing quote).

The matches are the names that start with the text typed. If there are none, case is ignored (smart case: a
lowercase letter matches either case, but an uppercase one only itself), so `mak` completes to `Makefile`. If there
are still none, the names that contain the text are used, so `conf` completes to `my.config`. Either way, the text
typed is replaced.

## Saving and restoring the shell's state

luish's own commands are subcommands of the `__luish_internal` built-in. `__luish_internal savestate` prints shell
commands that recreate the current state of the shell: the working directory, the file mode mask (`umask`),
variables (with their `export` and `readonly` attributes), traps, functions, aliases and options. Reading them back with `.` restores that state, in the same shell or in another:

```sh
__luish_internal savestate > ~/saved.sh
luish -c '. ~/saved.sh; myfunction'
```

Restoring adds to the current state: variables, functions and aliases defined since are kept. A variable that is
already `readonly` can't be restored, so reading the state back into the shell that saved it fails if it has any.
In `eval "$(__luish_internal savestate)"`, traps are lost, because a command substitution resets them.

## The version of luish

`__luish_internal print-git-rev` prints the git revision luish was built from, and `__luish_internal
print-git-rev-short` the same with the abbreviated hash. A build from sources with uncommitted changes adds `-dirty`,
and a build outside a git checkout prints `unknown`.

## Cached startup files

luish can cache the effects of your startup files, so that a new shell restores their result instead of running
them, which is much faster when they run slow commands. The files go in two directories, each used only if it exists
(they work like zsh's `.zshrc` and `.zlogin`):

- `~/.config/luish/rc.d/` (or `$XDG_CONFIG_HOME/luish/rc.d/`): for every interactive shell. This is the place for
  what isn't inherited by the shells you start: aliases, functions and options. Scripts and `luish -c` don't read it.
- `~/.config/luish/login.d/`: for login shells, after `rc.d`. Its files replace `/etc/profile` and `~/.profile`. This
  is the place for the environment: exported variables such as `PATH`, which the programs and shells you start
  inherit.

In each directory, the files whose names end in `.lsh` run in byte order. luish remembers what they did: the variables
they set, exported or unset, and their functions, aliases, options, traps and `umask`. An interactive shell then
reads `$ENV` and `luishrc`, uncached, as usual.

```sh
mkdir -p ~/.config/luish/login.d ~/.config/luish/rc.d
echo '. /etc/profile' > ~/.config/luish/login.d/00-system.lsh
echo 'export PATH=$HOME/bin:$PATH EDITOR=vim' > ~/.config/luish/login.d/10-env.lsh
echo "alias ll='ls -l'" > ~/.config/luish/rc.d/aliases.lsh
```

The caches are `~/.cache/luish/rc-HOST` and `~/.cache/luish/login-HOST` (or under `$XDG_CACHE_HOME`). A cache is
used as long as the `.lsh` files and the files they read with `.` are unchanged, and luish itself is the same build.
When one of them changes (luish compares their size and modification time), or after luish is upgraded, the next
shell reruns the files and updates the cache.

The cache directory holds nothing that can't be rebuilt: it can be removed at any time. luish marks it with a
`CACHEDIR.TAG` file, so that backup tools that honour the [convention](https://bford.info/cachedir/) skip it.

Some things belong in a directory's `_uncached.lsh`, which runs every time, after that directory's cached state is
restored:

- anything with side effects, such as starting `ssh-agent` or printing a message;
- values that differ between shells, such as `GPG_TTY=$(tty)`;
- anything that depends on the environment the shell was started in, such as `$SSH_CONNECTION` or `$DISPLAY`. The
  cached values are those from when the cache was built, so, for example, `PATH=$HOME/bin:$PATH` keeps the rest of
  the `PATH` that the shell had then. For the same reason, a file in `rc.d` shouldn't use variables set in
  `login.d` (in a login shell, `rc.d` runs before them; in the shells started from it, they are inherited).

Changes that luish can't see, such as newly installed software, the output of commands, or files tested with `[`,
don't refresh the caches: remove the cache files (or touch a `.lsh` file) after such changes.
