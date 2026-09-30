# `print`

```text
print [-abcDilmnNoOPrsz] [-C cols] [-f format] [-u fd] [-v name]
      [-x|-X tab-stop] [--] [arg...]
print -R [-en] [arg...]
```

Write the arguments, as zsh's `print` does.

`print` is a built-in only in interactive shells (and their subshells), so
that scripts find the same commands as they do in dash, where `print` may
be a program. `__luish_internal print` is the same command in any shell.

The arguments are written separated by spaces and followed by a newline.
Backslash escapes in them are expanded: `\a`, `\b`, `\e` (or `\E`), `\f`,
`\n`, `\r`, `\t`, `\v` and `\\`; `\NNN` (one to three octal digits) and
`\xNN` (one or two hex digits) for a byte; `\uNNNN` and `\UNNNNNNNN` for a
Unicode character, written as UTF-8; `\M-c` and `\C-c` for the meta and
control versions of a character (`\C-?` is DEL); and `\c`, which ends the
output there, without the newline. A backslash before any other character
is removed.

`-r`
: Don't expand backslash escapes.

`-R`
: Emulate BSD `echo`: don't expand escapes, and take only `-e` (do expand
  them) and `-n` as options in the words that follow, as `echo` does.

`-b`
: Also recognise the escapes of `bindkey`: `^c` is the control version of
  `c`. `\c` is then just `c`.

`-P`
: Expand `%` sequences as in prompts (see `setopt prompt.percent`), after
  the backslash escapes: `print -P '%F{red}%~%f'` writes the current
  directory in red.

`-D`
: Replace `$HOME` at the start of an argument with `~`.

`-m pattern`
: Keep only the arguments that match the pattern (the first argument).

`-o`, `-O`
: Sort the arguments in ascending or descending byte order; with `-i`,
  ignore case.

`-n`
: Don't write the newline at the end.

`-l`
: Separate the arguments by newlines instead of spaces.

`-N`
: Separate and end the arguments with NUL bytes (`-l` still separates
  them by newlines).

`-c`
: Write the arguments in columns, filled downwards, as many as fit in
  `$COLUMNS` (or the terminal's width, or 80 columns).

`-C cols`
: Write the arguments in `cols` columns.

`-a`
: With `-c` or `-C`, fill the rows across rather than the columns down.

`-x tab-stop`, `-X tab-stop`
: Expand the tabs at the start of each line (`-x`) or all tabs (`-X`) to
  spaces, with tab stops every `tab-stop` columns.

`-f format`
: Format the arguments as `printf` does. The arguments' escapes aren't
  expanded, but `-P` still applies.

`-u fd`
: Write to file descriptor `fd` instead of standard output.

`-v name`
: Set the variable `name` to the output instead of writing it, without the
  final newline.

`-s`
: Add the arguments, separated by spaces, to the history as a new entry,
  instead of writing them.

`-z`
: Start the next command line with the arguments, separated by spaces,
  instead of writing them. Several pushed lines come back in the reverse
  order.

Options can be grouped (`-lP`), and the argument of `-C`, `-f`, `-u`, `-v`,
`-x` and `-X` can follow the letter or be the next word. `-` or `--` ends
the options, and so does an argument that starts with `-` and a digit,
such as a negative number.

zsh's `-p` (write to the coprocess) always fails, as luish has no
coprocesses, and `-S` is not supported.

The exit status is 0, unless an option is invalid, `-u` names a file
descriptor that isn't open, or the variable of `-v` can't be set, which is
reported with status 1; or a conversion of `-f` failed, as with `printf`.
