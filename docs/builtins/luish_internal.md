# `__luish_internal`

```text
__luish_internal bindkey [arg...]
__luish_internal complete line
__luish_internal help [name...]
__luish_internal plugin load|list-loaded|list-available|unload [arg...]
__luish_internal print-git-rev
__luish_internal print-git-rev-short
__luish_internal savestate
```

Run one of luish's own commands.

luish's own commands are subcommands of this built-in, so that they don't
take names that scripts might use for something else.

`bindkey`
: Show or change the line editor's key bindings (see `help bindkey`).

`complete`
: Print what Tab offers for the last word of `line`, as the line editor
  would complete it (with the completers of loaded plugins), one match per
  line: the text that replaces the word, with the space or `/` that ends a
  single match, then a tab and the match's description, if it has one. The
  status is 1 if there are no matches, or if a completer failed. It works
  in any shell, for example to test a completer from a script.

`help`
: Show help for built-in commands (see `help help`).

`plugin`
: Load, list and unload plugins (see `help plugin`).

`print-git-rev`, `print-git-rev-short`
: Print the git revision luish was built from (the full or abbreviated
  hash), with `-dirty` if its sources differed from it, or `unknown` if it
  wasn't built from a git checkout.

`savestate`
: Print commands that recreate the state of the shell when read back with
  `.`: the working directory, the file mode mask, variables (with their
  `export` and `readonly` attributes), traps, functions, aliases, loaded
  plugins, key bindings changed with `bindkey`, and options. Restoring
  adds to the current state: nothing is unset.

A missing or unknown subcommand is an error with exit status 2.
