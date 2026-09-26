# `command`

```text
command [-p] command [argument...]
command [-p] -v | -V name
```

Run a command, bypassing functions, or describe it.

`command name args` runs `name` as a built-in or a program, even if a
function of the same name exists. Errors in special built-ins run this way
(such as a failed assignment in `command export`) don't exit the shell.

`-p`
: Search a default `PATH` that finds the standard utilities, instead of
  `$PATH`.

`-v`
: Print how `name` would be run: the path of a program, or just the name of
  a built-in, function or keyword, and an alias as the `alias` command that
  defines it. The exit status is 127 if it isn't found.

`-V`
: Describe `name` in words, as `type` does.
