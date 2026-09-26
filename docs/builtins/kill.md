# `kill`

```text
kill [-s signal | -signal] pid | job...
kill -l [status]
```

Send a signal to processes or jobs.

The signal (by default `TERM`) is given as a name, with or without `SIG`,
in any case (`-s hup`, `-HUP`), or as a number (`-9`). A negative `pid`
signals a process group. A job (see `help jobs`) is signalled as a whole.

`-l`
: List the signal names. With an exit status, show the name of the signal
  that caused it (`kill -l 130` shows `INT`).
