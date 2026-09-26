# `.`

```text
. file
```

Run the commands in a file in the current shell.

The commands can change the shell's variables, functions, options and
working directory, as if they had been typed. If `file` has no `/`, it is
searched for in `PATH` (the current directory is not searched unless it is
in `PATH`). A `return` in the file stops reading it.

The exit status is that of the last command run, or 0 if there was none. A
file that can't be read is an error, which exits a shell that isn't
interactive.
