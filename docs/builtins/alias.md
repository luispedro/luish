# `alias`

```text
alias [name[=value]...]
```

Define or show aliases.

`alias name=value` makes the word `name`, when it is the first word of a
command, stand for `value`. If `value` ends in a blank, the next word is
checked for an alias too. Without arguments, `alias` lists all aliases, in a
form that can be read back; `alias name` shows one. The exit status is 1 if
one of the names isn't an alias.

An alias takes effect from the next line that is read, not on the line that
defines it.
