# Usage

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`.

On the command line, `-o` and `+o` take any option named as for `setopt`. Case
and `_` don't matter, and a `no` prefix inverts it (`-o err_exit`, `-o
no_glob`, `-o prompt_percent`; `+o glob` is the same as
`-o noglob`). luish also has these long options:

| Option | Effect |
|---|---|
| `--login` | The same as `-l`: a login shell, which reads `/etc/profile` and `~/.profile` (or `login.d`, see below) |
| `--interactive` | The same as `-i`: an interactive shell, even when standard input is not a terminal |
| `--stdin` | The same as `-s`: read commands from standard input; the operands are the positional parameters |
| `--no-rcs` | Don't read any startup files: `config.toml`, `rc.d`, `$ENV`, `luishrc`, and for a login shell `login.d` or `/etc/profile` and `~/.profile`. As zsh's `--no-rcs` |
| `--no-plugins` | Load no plugins: those that `config.toml` enables, and `plugin load` does nothing (as do `plugin sync`, `plugin update` and `plugin check`), for example to check whether a problem comes from a plugin. The startup caches are neither used nor written |
| `--help` | Show a summary of the options and exit |
| `--version` | Show the version of luish and the git revision it was built from, and exit |

Options end at the first operand, or at `--` or `-`.

## Getting help

In an interactive shell, `help` lists the built-in commands, and `help NAME` shows the help for any of them (the
same text as in [](builtins.md)).

## Prompts

`PS1` is the prompt, `PS2` the prompt for the continuation lines of a command, and `PS4` the prefix of the lines
that `set -x` prints. As POSIX requires, they go through parameter expansion, so `PS1='$PWD\$ '` shows the current
directory.

With the `prompt.percent` option (`setopt prompt.percent`), they then also expand `%` sequences, as in zsh:

```sh
setopt prompt.percent
PS1='%[fg:blue]%[dir]%[fg_off] %([status]..%[fg:red][%[status]]%[fg_off] )%[prompt_char] '
```

This shows the current directory (with `~` for `$HOME`) in blue, then the exit status of the last command in red if
it failed, then `#` for root and `%` for other users. Parameter expansion comes first, so a `%` in the value of a
variable is expanded too; write `%%` for a literal `%`. The sequences are those of zsh:

| Sequence | Expands to | Long name |
|---|---|---|
| `%%`, `%)` | `%`, `)` | `%[percent]` |
| `%~`, `%d` or `%/` | The current directory, with or without `~` for `$HOME`. With a number, `%N~` gives only its last `N` components, and `%-N~` its first `N` | `%[dir]`, `%[pwd]` |
| `%c` or `%.`, `%C` | The last component of the current directory, with or without `~` (`%Nc` for more) | `%[dir_tail]`, `%[pwd_tail]` |
| `%n`, `%m`, `%M` | The user name, the host name up to the first `.` (`%Nm`: `N` components), the full host name | `%[user]`, `%[host]`, `%[hostname]` |
| `%#` | `#` for root, `%` otherwise | `%[prompt_char]` |
| `%?` | The exit status of the last command | `%[status]` |
| `%h` or `%!` | The number of the next history event | `%[history]` |
| `%j` | The number of jobs | `%[jobs]` |
| `%L`, `%i` | `$SHLVL`, the line number (for `PS4`) | `%[shlvl]`, `%[lineno]` |
| `%l`, `%y` | The terminal, without `/dev/` (and, for `%l`, without `tty`) | `%[tty_short]`, `%[tty]` |
| `%D`, `%T`, `%*`, `%t` or `%@`, `%w`, `%W` | The date as `yy-mm-dd`, the time as `HH:MM` or `HH:MM:SS`, or in 12-hour format, the weekday and day, the date as `mm/dd/yy` | `%[date]`, `%[time]`, `%[time_seconds]`, `%[time_12h]`, `%[date_weekday]`, `%[date_us]` |
| `%D{format}` | The time in a `strftime` format (and zsh's `%f`, `%K` and `%L`, the day and hours without padding) | `%[date:format]` |
| `%B` `%b`, `%U` `%u`, `%S` `%s` | Start and stop bold, underline and standout (reverse video) | `%[bold]` `%[bold_off]`, `%[underline]` `%[underline_off]`, `%[standout]` `%[standout_off]` |
| `%F{colour}` `%f`, `%K{colour}` `%k` | Start and stop a foreground and a background colour: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, a number from 0 to 255, or `#rrggbb`. `%NF` is `%F{N}` | `%[fg:colour]` `%[fg_off]`, `%[bg:colour]` `%[bg_off]` |
| `%E` | Clear to the end of the line | `%[clear_eol]` |
| `%{...%}` | Text written as it is, taking no room on the screen: for other escape sequences, such as a terminal title | |
| `%NG` | Within `%{...%}`: the escape sequence takes `N` columns | |
| `%(x.yes.no)` | `yes` if the condition `x` holds, otherwise `no` (any character can replace the `.`s). The conditions take a number `N`, as in `%(N?.yes.no)` or `%N(?.yes.no)`: `?` the exit status is `N` (0 by default), `#` the user id is `N` (0: root), `!` the shell runs as root, `g` the group id is `N`, `j` there are at least `N` jobs, `L` `$SHLVL` is at least `N`, `/` or `C` the current directory has at least `N` components, `~`, `.` or `c` the same, with `~` for `$HOME` counting as one; `T`, `t`, `d`, `D` and `w`: the hour, minute, day of the month, month (from 0 for January) or day of the week (from 0 for Sunday) is `N` | `%([name].yes.no)` |
| `%N<text<`, `%N>text>` | Shorten what follows (up to the end of the enclosing `%(...)`, or to the next `%<<`) to `N` characters, replacing what is cut on the left or the right by `text` | |

Other sequences expand to nothing. zsh's `%_`, `%e`, `%I`, `%N`, `%x`, `%v`, and conditions and truncation widths
relative to the terminal's width, aren't supported.

The long names are luish's own (zsh has none): `%[name]` is the same as the short sequence, easier to read in a long
prompt. The argument of a sequence goes after a `:`, as a number or as the text in braces: `%[dir:2]` is `%2~`,
`%[fg:red]` is `%F{red}` and `%[date:%H:%M]` is `%D{%H:%M}` (a `\` quotes a `]`). The number can also come first, as
in `%2[dir]`. Case, `_` and `-` don't matter, so `%[HostName]` is `%[hostname]`.

The conditions of `%(...)` have long names too, in brackets, with their number after a `:`: `%([status:1].yes.no)`
is `%(1?.yes.no)`. They are `status` (`?`), `root` (`!`), `uid` (`#`), `gid` (`g`), `jobs` (`j`), `shlvl` (`L`),
`pwd` (`/`), `dir` (`~`), `hour` (`T`), `minute` (`t`), `day` (`d`), `month` (`D`) and `weekday` (`w`).

Unlike an unknown short sequence, an unknown long name is an error, written to stderr each time the prompt is
expanded (it then expands to nothing). It suggests the closest name, or else lists them:

```text
luish: unknown prompt sequence %[hostnam]; did you mean %[hostname]?
```

zsh's deprecated form of truncation, `%[N<text]` (a `[` followed by a number or by `<` or `>`), is `%N<text<`.

For what `PS1` can't compute by itself, such as the git branch, a plugin can provide variables for it to use, which
are set only while the prompt is built (see [Customizing the prompt](plugins.md#customizing-the-prompt)).

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

With `setopt editor.autosuggest`, the line editor suggests the rest of the newest command in the history that starts
with what has been typed, in grey after the cursor, as the zsh-autosuggestions plugin does. Right, End, Ctrl-F or
Ctrl-E accept the suggestion, and Alt-F accepts its next word. Its colour is the `suggest` entry of
`$LUISH_HIGHLIGHT` (see [Syntax highlighting](#syntax-highlighting); by default grey, `90`).

To bind Up and Down as zsh does by default:

```sh
bindkey Up up-line-or-history
bindkey Down down-line-or-history
```

Keys can be named (as `Up`, `Ctrl-Right`, `Alt-.` or `'Ctrl-X Ctrl-E'`) or written as zsh writes them (`'^[[A'`),
and bindings can also go in `config.toml` (see below).

## Syntax highlighting

The line editor colours the command line as it is typed.

`$LUISH_HIGHLIGHT` sets the colours, as a list of `class=SGR` entries separated by `:` (as in `GREP_COLORS`), where
SGR is the parameters of a terminal escape sequence. Its entries replace these defaults:

```text
keyword=1;34:command=32:unknown=1;31:string=33:var=36:subst=35:op=1:redir=1:comment=90:assign=34:select=7:desc=90:suggest=90
```

`select` and `desc` are for the completion menu, and `suggest` for autosuggestions. An empty SGR leaves a class
uncoloured. `LUISH_HIGHLIGHT=none`, or a non-empty `$NO_COLOR`, turns highlighting off.

## History

An interactive shell keeps the last `HISTSIZE` commands (1000 by default) in
its history, where the line editor (Up and Down, Ctrl-R, Alt-.) and `fc` find
them.

A command the same as the one before it is not added again. By default, history
is saved to `$XDG_STATE_HOME/luish/history`; set `HISTFILE` to a different
value to change that or to an empty value to keep no file.

You can set the history options in `config.toml` (see [Settings in
config.toml](#settings-in-configtoml)), or with

```sh
setopt -p history \
    file=~/.histfile \
    save_size=10000 \
    share \
    ignore_space \
    reduce_blanks
```

The above example also demonstrates the `-p` option of `setopt` (for prefix),
which enables setting options in a group. It is equivalent to

```
setopt history.file=~/.histfile
setopt history.save_size=10000
setopt history.share
setopt history.ignore_space
setopt history.reduce_blanks
```


Unlike zsh, luish saves the history by default: zsh keeps no file unless
`HISTFILE` and `SAVEHIST` are set.

## Tab completion

In an interactive shell, Tab starts completion.


- `$` completes variable names, and `${` also function names;
- `cd`, `pushd` and `rmdir` complete directories; for `cd` and `pushd`, when none in the current directory match,
  the directories in `CDPATH` complete instead (listed with the `CDPATH` directory they are in), as in zsh;
- `export`, `local`, `readonly`, `unset`, `read` (except the prompt after `-p`), `getopts` (after the option
  string) and `for` (then `in`) complete variable names (`unset -f` completes function names);
- `alias` and `unalias` complete aliases;
- `type`, `hash` and `which` complete command names, and `help` completes built-ins;
- `fg`, `bg`, `jobs`, `wait` and `kill` complete job specs such as `%1`, listed with their commands (after `%` and
  a letter, they complete the command names instead, such as `%vim`);
- `kill -` and `kill -s` complete signal names, as do the arguments of `trap` after its action;
- `setopt` completes the options that are off and `unsetopt` those that are on,
  as in zsh.
- `plugin load` completes the plugins in the plugin directory, and `plugin unload` the loaded ones.

Plugins, through their extensions, can provide completion for other commands
(see [Plugins](plugins.md)). Aliases are followed: if `g` is an alias for
`git`, then `g ` completes as `git ` does.

As in zsh, a word with a glob, a `$` or a command substitution in it is expanded instead: `ls *.md` Tab becomes
`ls a.md b.md c\ d.md ` (quoted, and followed by a space when there are several words), and `echo $HOME` Tab becomes
`echo /home/me`. A glob that matches nothing, or a variable that is empty or unset, is completed as usual (so
`$HO` Tab still completes variable names).

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


## Settings in `config.toml`

luish's settings can also be set in `~/.config/luish/config.toml` (or
`$XDG_CONFIG_HOME/luish/config.toml`), a [TOML](https://toml.io) file.


```toml
[options]
autosuggest = true

[options.history]
file = "~/.histfile"
save_size = 10000
share = true
ignore_space = true

[options.glob]
star = true
```

The values have TOML's types: `true` or `false` for an option, an integer for a
number, and a string for text, where a leading `~` is expanded to the home
directory (nothing else in it is expanded).

Aliases go in the `alias` table, each as `NAME = "VALUE"`, the same as `alias
NAME=VALUE`. Global and suffix aliases (`alias -g` and `alias -s`, see `help
alias`) go in its `global` and `suffix` tables:

```toml
[alias]
ll = "ls -l"
".." = "cd .."            # a name with other characters than letters, digits, _ and - is quoted

[alias.global]
G = "| grep"              # ls G foo runs ls | grep foo
"..." = "../.."

[alias.suffix]
pdf = "evince"            # notes.pdf runs evince notes.pdf
```

A string named `global` or `suffix` in `[alias]` is an ordinary alias with that
name (but TOML doesn't allow it in the same file as the table of that name).
Values are used as they are, with no `~` expansion.

Key bindings can be set in the `bindkey` table, each as `KEY = "WIDGET"`, the
same as `bindkey KEY WIDGET` (see `help bindkey`):

```toml
[bindkey]
Up = "up-line-or-history"
Down = "down-line-or-history"
"Ctrl-X Ctrl-E" = "undo"
"^[[1;5C" = "forward-word"   # Ctrl-Right, as zsh writes it
```

A key that isn't a setting, a value of the wrong type, an alias name with `=`
in it or a key or widget that `bindkey` doesn't take is reported with its line
and skipped; a file that isn't valid TOML is reported and ignored.

You also [install plugins in
config.toml](plugins.md#installing-plugins-with-configtoml)).

To keep the same settings on several machines, put them in a plugin of your own
(see [a personal plugin](plugins.md#suggestion-a-personal-plugin)) and share that
across them with git or another tool.

## Cached startup files

luish caches the effects of your startup files, so that a new shell can restore
their result instead of running them, which is much faster when they run slow
commands. The files go in two directories, each used only if it exists
(they work like zsh's `.zshrc` and `.zlogin`):

- `~/.config/luish/rc.d/` (or `$XDG_CONFIG_HOME/luish/rc.d/`): for every
  interactive shell, after `config.toml`. This is the place for what isn't
  inherited by the shells you start: aliases, functions and options. Scripts
  and `luish -c` don't read it.
- `~/.config/luish/login.d/`: for login shells, after `rc.d`. Its files replace
  `/etc/profile` and `~/.profile`. This is the place for the environment:
  exported variables such as `PATH`, which the programs and shells you start
  inherit.

In each directory, the files whose names end in `.lsh` run in byte order. luish
remembers what they did: the variables they set, exported or unset, and their
functions, aliases, options, traps and `umask`. An interactive shell then reads
`$ENV` and `luishrc`, uncached, as usual.

```sh
mkdir -p ~/.config/luish/login.d ~/.config/luish/rc.d
echo '. /etc/profile' > ~/.config/luish/login.d/00-system.lsh
echo 'export PATH=$HOME/bin:$PATH EDITOR=vim' > ~/.config/luish/login.d/10-env.lsh
echo "alias ll='ls -l'" > ~/.config/luish/rc.d/aliases.lsh
```

The caches are `~/.cache/luish/rc-HOST` and `~/.cache/luish/login-HOST` (or
under `$XDG_CACHE_HOME`). A cache is used as long as the `.lsh` files, the
files they read with `.` and (for `rc.d`) `config.toml`, `plugins.lock` and the
plugins it loads are unchanged, and luish itself is the same build.

When one of them changes (luish compares their size and modification time), or
after luish is upgraded, the next shell reruns the files and updates the cache.

The cache directory holds nothing that can't be rebuilt: it can be removed at
any time. luish marks it with a `CACHEDIR.TAG` file, so that backup tools that
honour the [convention](https://bford.info/cachedir/) skip it.

For elements that cannot be cached, put them into a file called
`_uncached.lsh`, which runs every time, after that directory's cached state is
restored. For example:

- anything with side effects, such as starting `ssh-agent` or printing a message;
- values that differ between shells, such as `GPG_TTY=$(tty)`;
- anything that depends on the environment the shell was started in, such as
  `$SSH_CONNECTION` or `$DISPLAY`. The cached values are those from when the
  cache was built, so, for example, `PATH=$HOME/bin:$PATH` keeps the rest of
  the `PATH` that the shell had then.

The system is not perfect: luish can't see what a command does, so it can't
know whether it changed the environment. In the future, luish may use more
sophisticated ways to detect changes, but for now, it only checks the files it
reads.


## Internals

luish's own commands are subcommands of the `__luish_internal` built-in.

### Saving and restoring the shell's state

`__luish_internal savestate` prints shell commands that recreate the current
state of the shell: the working directory, the file mode mask (`umask`),
variables (with their `export` and `readonly` attributes), traps, functions,
aliases and options. Reading them back with `.` restores that state, in the
same shell or in another:

```sh
__luish_internal savestate > ~/saved.sh
luish -c '. ~/saved.sh; myfunction'
```

Restoring adds to the current state: variables, functions and aliases defined since are kept. A variable that is
already `readonly` can't be restored, so reading the state back into the shell that saved it fails if it has any.
In `eval "$(__luish_internal savestate)"`, traps are lost, because a command substitution resets them.

### The version of luish

`__luish_internal print-git-rev` prints the git revision luish was built from, and `__luish_internal
print-git-rev-short` the same with the abbreviated hash. A build from sources with uncommitted changes adds `-dirty`,
and a build outside a git checkout prints `unknown`.
