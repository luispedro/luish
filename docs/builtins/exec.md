# `exec`

```text
exec [command [argument...]]
```

Replace the shell with a command, or redirect the shell.

With a command, the shell process becomes that command, which is searched
for in `PATH` (functions and built-ins are not used). A command that can't
be run exits the shell with status 126 or 127.

Without a command, the redirections on `exec` apply to the shell itself
from then on:

```sh
exec 3>log.txt      # open fd 3 for the rest of the script
exec >&3 2>&1       # send all output there
```
