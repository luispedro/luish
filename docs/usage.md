# Usage

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`.

## Saving and restoring the shell's state

luish's own commands are subcommands of the `__luish_internal` built-in. `__luish_internal savestate` prints shell
commands that recreate the current state of the shell: the working directory, the file mode mask (`umask`),
variables (with their `export` and `readonly` attributes), traps, functions, aliases and options. Reading them back with `.` restores that state, in the same shell or in another:

```sh
__luish_internal savestate > ~/saved.sh
luish -c '. ~/saved.sh; myfunction'
```

Restoring adds to the current state: variables, functions and aliases defined since are kept. A variable that is
already `readonly` can't be restored, so reading the state back into the shell that saved it fails if it has any.
In `eval "$(__luish_internal savestate)"`, traps are lost, because a command substitution resets them.
