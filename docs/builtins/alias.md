# `alias`

```text
alias [{+|-}gmrsL] [name[=value]...]
```

Define or show aliases.

`alias name=value` makes the word `name`, when it is the first word of a
command, stand for `value`. If `value` ends in a blank, the next word is
checked for an alias too. Without arguments, `alias` lists the regular and
global aliases, in a form that can be read back; `alias name` shows one. The
exit status is 1 if one of the names isn't an alias.

An alias takes effect from the next line that is read, not on the line that
defines it.

The options are based on `zsh`'s `alias` command:

- `-g` defines a **global** alias, which is expanded wherever it is a word
  of its own, not only as a command name: after `alias -g G='| grep'`,
  `ls G foo` runs `ls | grep foo`. It isn't expanded when quoted (`'G'` or
  `\G`), as a here-document delimiter, or as a reserved word.
- `-s` defines a **suffix** alias: a command name that ends in `.` and the
  suffix, with something before the `.`, is run with the alias's value in
  front. After `alias -s pdf=evince`, the command `notes.pdf` runs
  `evince notes.pdf`.
- `-r`, `-g` and `-s` select regular, global or suffix aliases for listing
  (only `-s` lists suffix aliases, which have a table of their own).
- `-m` takes the names as patterns (quote them), and lists the aliases that
  match.
- `-L` lists aliases as `alias` commands, with `-g` or `-s` as needed.
- `+` instead of `-` (or a `+` on its own after the options) lists the
  names only.

Since an argument that starts with `-` or `+` is an option, put `--` before
a name that starts with one: `alias -- -x='...'`.

Aliases can also be defined in `config.toml` (see "Settings in
`config.toml`" in the user guide).
