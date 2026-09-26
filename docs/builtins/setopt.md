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

The exit status is 1 if an option doesn't exist or can't be changed
(`interactive` and `stdin`); the other options given are still set.
