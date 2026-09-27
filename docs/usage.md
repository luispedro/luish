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

## Line editing

An interactive shell edits command lines with emacs keys, as zsh does, or with vi keys after `set -o vi` (or
`bindkey -v`). The emacs keys are zsh's, and `bindkey` shows and changes them (see `help bindkey`). Some of the most
useful:

| Key | Action |
|---|---|
| Up, Down | The previous or next command that starts with the text before the cursor (all commands if the line is empty); Down past the newest brings back what was typed |
| Ctrl-P, Ctrl-N | The previous or next command |
| Ctrl-R | Search the history as you type |
| Alt-. | Insert the last word of the previous command; again, that of the one before |
| Ctrl-O | Run the line, and start the next one with the command after it in the history, to run a series of commands again |
| Ctrl-W, Alt-Backspace | Delete the word before the cursor |
| Alt-B, Alt-F, Alt-D | Move back a word, forward to the next word, or delete to the end of the word |
| Ctrl-A, Ctrl-E | Go to the start or end of the line |
| Ctrl-K, Ctrl-U | Delete to the end of the line, or the whole line |
| Ctrl-Y, Alt-Y | Put back what was deleted, or instead what was deleted before it |
| Ctrl-_ | Undo |

As in zsh, words are made of letters, digits and the characters in `WORDCHARS`, by default
`*?_-.[]~=/&;!#$%^(){}<>`. Many zsh users leave out `/`, so that Ctrl-W deletes one directory of a path:

```sh
WORDCHARS='*?_-.[]~=&;!#$%^(){}<>'
```

To bind Up and Down as zsh does by default:

```sh
bindkey '^[[A' up-line-or-history
bindkey '^[[B' down-line-or-history
```

## History

An interactive shell keeps the last `HISTSIZE` commands (1000 by default) in its history, where the line editor
(Up and Down, Ctrl-R, Alt-.) and `fc` find them. A command the same as the one before it is not added again. The history is
saved to the file `HISTFILE`, by default `$XDG_STATE_HOME/luish/history` (`~/.local/state/luish/history` if
`XDG_STATE_HOME` isn't set); set `HISTFILE` to an empty value to keep no file. The file is read after the startup
files, so `HISTFILE` and `HISTSIZE` can be set there (or in `luishrc`).

The file is in zsh's format, with the time of each command, so luish and zsh can use the same file:

```sh
HISTFILE=~/.histfile
SAVEHIST=10000
setopt share_history hist_ignore_space hist_reduce_blanks
```

The file keeps up to `SAVEHIST` commands (by default as many as `HISTSIZE`). New commands are added to the end of it
when the shell exits, so several shells don't overwrite each other's commands, and when it has grown to more than
20% over `SAVEHIST` the oldest commands are dropped. Some options change this, as in zsh (see `help setopt`):

- `inc_append_history` adds each command to the file as soon as it is run;
- `share_history` does too, and also reads the commands that other shells have added before each prompt, so that
  commands typed in one terminal can be recalled in all of them;
- `hist_ignore_space` keeps commands that start with a space out of the file (the next command replaces them in
  memory as well), which is useful for commands with passwords;
- `hist_reduce_blanks` removes superfluous blanks from commands;
- `hist_save_no_dups` drops older copies of repeated commands when the file is trimmed.

Unlike zsh, luish saves the history by default: zsh keeps no file unless `HISTFILE` and `SAVEHIST` are set.

## Tab completion

In an interactive shell, Tab completes the word under the cursor: a command name (a built-in, function, alias or
program in `PATH`) at the start of a command, a variable name after `$` or `${`, a user's home directory after `~`,
and a filename elsewhere, also after the `=` or `:` of an assignment and the `=` of a `--option=`. After commands
that run another command, such as `sudo`, `env`, `nohup`, `time` and `xargs`, the command they run (after their
options) completes as a command name. With `setopt autocd`, a command name also completes to directories (see
`help setopt`). Some commands complete their arguments differently:

- `cd`, `pushd` and `rmdir` complete directories; for `cd` and `pushd`, when none in the current directory match,
  the directories in `CDPATH` complete instead (listed with the `CDPATH` directory they are in), as in zsh;
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

The first Tab completes as much as is common to all the matches. When there is nothing more to add, it opens a menu
of the matches below the line (see below). What is added is quoted as needed: a file called `my file` is completed as `my\ file`, or as `'my file'` after a `'`. A directory is completed
with a `/`, so Tab can go on into it; any other single match gets a space (and the closing quote).

The matches are the names that start with the text typed. If there are none, case is ignored (smart case: a
lowercase letter matches either case, but an uppercase one only itself), so `mak` completes to `Makefile`. If there
are still none, the names that contain the text are used, so `conf` completes to `my.config`. Either way, the text
typed is replaced.

### The completion menu

The menu shows the matches in columns, or one per line with their descriptions (such as the commands of jobs for
`fg`). If it doesn't fit on the screen, it scrolls, and its last line says which rows are shown. The next Tab selects
the first match and puts it in the line, and then:

| Key | Action |
|---|---|
| Tab, Shift-Tab | Select the next or the previous match |
| Arrow keys, Ctrl-N, Ctrl-P, Ctrl-F, Ctrl-B | Move down, up, right or left in the menu |
| Page Down, Page Up | Move a screenful down or up |
| Enter | Keep the match and close the menu |
| Esc, Ctrl-G | Put back the text typed and close the menu |

Any other key keeps the match, closes the menu and does what it usually does, so you can type on after it. Before a
match is selected, Down, Ctrl-N and Shift-Tab also start selecting (Shift-Tab from the last match), but the other keys
do what they usually do: Enter runs the command, and Up goes back in the history.

Esc on its own takes effect after 0.4 seconds in emacs mode (0.1 seconds in vi mode), since until then it could be
the start of a Meta key (`Esc` then `b` is Meta-B). In vi mode, a second Esc goes to command mode. The selected match is shown in reverse video, and descriptions in grey. They can be
changed with the `select` and `desc` entries of `$LUISH_HIGHLIGHT`, a list of `class=SGR` entries separated by `:`,
where SGR is the parameters of a terminal escape sequence (`select=1;33:desc=` shows the selection in bold yellow
and the descriptions uncoloured).

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
