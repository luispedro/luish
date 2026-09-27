# `setopt`, `unsetopt`

```text
setopt [option[=value]...]
unsetopt [option...]
```

Turn options on or off by name, and set settings.

These commands are not in POSIX; they work as in zsh. They set the options
that `set` does, by their long names (see `set`), and also luish's own
options, which `set -o` doesn't show or accept, so that it behaves as in
dash. luish's own options have names in groups, such as `history.share`;
their earlier names (such as `sharehistory`) and zsh's (`share_history`)
also work. Case doesn't matter and `_` is ignored, so `PROMPT_PERCENT` and
`promptpercent` are the same option. A `no` prefix inverts a name:
`setopt no_err_exit` is `unsetopt errexit`, `unsetopt glob` is
`setopt noglob`, and `setopt history.no_share` is
`unsetopt history.share`.

`setopt option=value` sets an option on (`true`, `on`, `yes` or `1`) or
off (`false`, `off`, `no` or `0`), or sets a setting that has a value (see
below). As for `export`, an argument of this form is expanded as an
assignment: a `~` after the `=` (or after a `:`) is expanded, and the
value isn't split into fields, so `setopt history.file=~/.history` works.

Without arguments, `setopt` lists the options that are on and `unsetopt`
the options that are off, in alphabetical order.

luish's own options, all off by default:

`prompt.percent` (`promptpercent`)
: Expand `%` sequences in `PS1`, `PS2` and `PS4`, after parameter
  expansion, as zsh does (see the user documentation on prompts).

`glob.star` (`globstar`)
: `**/` in a pattern matches any number of directories, as in zsh.

`glob.bare_qualifiers` (`bareglobqual`)
: Parentheses at the end of a pattern hold zsh's glob qualifiers, as in
  `*(/)` (directories) or `*(.om[1])` (the newest file). See the
  documentation on extended globbing.

`cd.auto` (`autocd`)
: A command that is only a directory's name, with no arguments or
  redirections, changes to that directory, as in zsh. It applies only to
  commands read from standard input (as in an interactive shell), not to
  scripts or `-c`, and only when there is no command, function or
  executable file of that name. A relative name not starting with `.` or
  `..` is looked for in the current directory, then in `CDPATH`.

`pushd.auto`, `pushd.ignore_dups`, `pushd.silent`
: zsh's `autopushd`, `pushdignoredups` and `pushdsilent`: `cd` pushes the
  previous directory onto the directory stack (and takes `+n` and `-n`);
  the new directory is removed from the stack after `cd`, `pushd` and
  `popd`, so that each is there once; `pushd` and `popd` don't print the
  stack. See `pushd`.

`editor.autosuggest` (`autosuggest`)
: In an interactive shell, show the rest of the newest command in the
  history that starts with the line typed, in grey after the cursor, as
  zsh-autosuggestions does. Right, End, Ctrl-F or Ctrl-E accept it, and
  Alt-F accepts its next word.

The history options (see the user documentation on history):

`history.ignore_space` (`histignorespace`)
: Don't save a command that starts with a space or a tab. It stays in the
  history, to be recalled, until the next command replaces it.

`history.reduce_blanks` (`histreduceblanks`)
: Replace runs of blanks in a command by one space before it goes into
  the history, except in quotes and here-documents.

`history.save_no_dups` (`histsavenodups`)
: When the history file is trimmed, leave out commands that are repeated
  later in it.

`history.inc_append` (`incappendhistory`)
: Append each command to the history file when it is run, rather than
  when the shell exits.

`history.share` (`sharehistory`)
: As `history.inc_append`, and also read the commands that other shells
  have added to the history file, before each prompt.

Settings with a value, which are the shell variables in parentheses:
`setopt name=value` sets the variable (a number must be a non-negative
decimal number), and `unsetopt name` unsets it, which gives the default
back. They aren't listed by `setopt` or `unsetopt` without arguments.

`history.file` (`HISTFILE`)
: The history file, by default `$XDG_STATE_HOME/luish/history`. An empty
  value means no file.

`history.size` (`HISTSIZE`)
: How many commands the history keeps, 1000 by default.

`history.save_size` (`SAVEHIST`)
: How many commands the history file keeps, by default as many as
  `history.size`.

The exit status is 1 if an option doesn't exist or can't be changed
(`interactive` and `stdin`), if a value is wrong or missing, if
`unsetopt` is given a value, or if the variable of a setting is
read-only; the other options given are still set.
