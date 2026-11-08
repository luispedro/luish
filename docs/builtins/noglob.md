# `noglob`

```text
noglob command [argument...]
```

Run a command without globbing its arguments.

`noglob` as the first word of a command turns off pathname expansion for the
words after it, as in zsh, so `noglob echo *.c` prints `*.c` whether or not a
file matches. The other expansions are still made (braces, `~`, parameters,
`$(...)` and field splitting), and the commands inside a `$(...)` glob as
usual: only the words of this command are left as they are. It is not in
POSIX.

`noglob` is a precommand modifier: it acts when the first word of a command
expands to it (also after `builtin`, and from a variable, as in zsh), and is
then left out of the command, so `set -x` doesn't show it. An alias is how it
is usually given, `alias mmv='noglob zmv -W'`, so that `mmv *.c *.h` passes
the patterns to the command. It can come before `command`, `builtin` and
`exec`. After `command`, it runs its arguments as a command, functions
included, after they were globbed.
