# `builtin`

```text
builtin [name [argument...]]
```

Run a built-in, bypassing functions.

`builtin name args` runs the built-in `name`, even if a function of the same
name exists, as in zsh and bash. It is not in POSIX. If there is no such
built-in, it prints an error and its status is 1: unlike `command`, it never
runs a program. A special built-in run this way stays special, so its
errors (such as `builtin set -o bogus`) exit a non-interactive shell.
It also runs the commands that plugins add in Rhai (`sh::builtin`).
