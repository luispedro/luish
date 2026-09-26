# `hash`

```text
hash [-r] [name...]
```

Remember or forget where commands are found.

The shell remembers where it found each command in `PATH`. Without
arguments, `hash` prints the remembered paths. With names, it looks each one
up in `PATH` and remembers it (the exit status is 1 if one isn't found).

`-r`
: Forget all remembered paths.

Assigning `PATH` also forgets them. An interactive luish forgets them by
itself when a directory in `PATH` changes, so `hash -r` isn't needed after
installing a command.
