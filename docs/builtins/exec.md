# `exec`

```text
exec [-cl] [-a name] [command [argument...]]
```

Replace the shell with a command, or redirect the shell.

With a command, the shell process becomes that command, which is searched
for in `PATH` (functions and built-ins are not used). A command that can't
be run exits the shell with status 126 or 127.

The options, as in zsh and bash, change how the command is run:

- `-c`: with an empty environment, not even the assignments before `exec`.
- `-a name`: with `name` as its `argv[0]`, the name it sees itself called by
  (`$0` in a shell). The command is still looked up by its own name.
- `-l`: with `-` before its `argv[0]`, which tells a shell that it is a login
  shell, as in `exec -l $SHELL`. It does nothing together with `-a` (as in
  zsh; bash puts the `-` before `name`).

They need a command: without one, an option is an error.

Without a command, the redirections on `exec` apply to the shell itself
from then on:

```sh
exec 3>log.txt      # open fd 3 for the rest of the script
exec >&3 2>&1       # send all output there
```
