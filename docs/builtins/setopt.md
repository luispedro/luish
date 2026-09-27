# `setopt`, `unsetopt`

```text
setopt [option...]
unsetopt [option...]
```

Turn options on or off by name, as in zsh.

These commands are not in POSIX; they work as in zsh. They set the options
that `set` does, by their long names (see `set`), and also luish's own
options, which `set -o` doesn't show or accept, so that it behaves as in
dash. Option names are as in zsh: case doesn't matter and `_` is ignored,
so `PROMPT_PERCENT` and `promptpercent` are the same option. A `no` prefix
inverts a name: `setopt no_err_exit` is `unsetopt errexit`, and
`unsetopt glob` is `setopt noglob`.

Without arguments, `setopt` lists the options that are on and `unsetopt`
the options that are off, in alphabetical order.

luish's own options, all off by default:

`promptpercent`
: Expand `%` sequences in `PS1`, `PS2` and `PS4`, after parameter
  expansion, as zsh does (see the user documentation on prompts).

`globstar`
: `**/` in a pattern matches any number of directories, as in zsh.

`bareglobqual`
: Parentheses at the end of a pattern hold zsh's glob qualifiers, as in
  `*(/)` (directories) or `*(.om[1])` (the newest file). See the
  documentation on extended globbing.

`autocd`
: A command that is only a directory's name, with no arguments or
  redirections, changes to that directory, as in zsh. It applies only to
  commands read from standard input (as in an interactive shell), not to
  scripts or `-c`, and only when there is no command, function or
  executable file of that name. A relative name not starting with `.` or
  `..` is looked for in the current directory, then in `CDPATH`.

`autosuggest`
: In an interactive shell, show the rest of the newest command in the
  history that starts with the line typed, in grey after the cursor, as
  zsh-autosuggestions does. Right, End, Ctrl-F or Ctrl-E accept it, and
  Alt-F accepts its next word.

The history options (see the user documentation on history):

`histignorespace`
: Don't save a command that starts with a space or a tab. It stays in the
  history, to be recalled, until the next command replaces it.

`histreduceblanks`
: Replace runs of blanks in a command by one space before it goes into
  the history, except in quotes and here-documents.

`histsavenodups`
: When the history file is trimmed, leave out commands that are repeated
  later in it.

`incappendhistory`
: Append each command to the history file when it is run, rather than
  when the shell exits.

`sharehistory`
: As `incappendhistory`, and also read the commands that other shells
  have added to the history file, before each prompt.

The exit status is 1 if an option doesn't exist or can't be changed
(`interactive` and `stdin`); the other options given are still set.
