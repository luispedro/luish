# `__luish_internal`

```text
__luish_internal help [name...]
__luish_internal plugin load|list|unload [arg...]
__luish_internal print-git-rev
__luish_internal print-git-rev-short
__luish_internal savestate
```

Run one of luish's own commands.

luish's own commands are subcommands of this built-in, so that they don't
take names that scripts might use for something else.

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
  plugins and options. Restoring adds to the current state: nothing is unset.

A missing or unknown subcommand is an error with exit status 2.
