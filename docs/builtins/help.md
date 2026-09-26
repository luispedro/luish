# `help`

```text
help [name...]
```

Show help for built-in commands.

Without arguments, `help` lists the built-ins, each with a one-line summary.
With names, it shows the help for each of them. The exit status is 1 if
there is no help for one of the names.

`help` is a built-in only in interactive shells (and their subshells), so
that scripts find the same commands as in other shells, where `help` may be
something else. Anywhere, `__luish_internal help` does the same.
