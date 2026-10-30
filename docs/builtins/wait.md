# `wait`

```text
wait [-fn] [-p var] [job | pid...]
```

Wait for background jobs to finish.

Without arguments, `wait` waits for all background jobs, and its exit
status is 0. With arguments, it waits for each of them in turn (a process
ID, or a job as described in `help jobs`), and the exit status is that of
the last one, or 127 if it isn't a child of the shell.

A signal that has a trap interrupts the wait: the exit status is 128 plus
the signal number, and the trap runs with `$?` already set to it.

The options are bash's (not POSIX):

`-n`
: Wait for one job to finish, any of them or the first of those named, and
  return its status. A job that has already finished (and hasn't been
  waited for) counts, the one with the lowest number first. A pid that isn't
  a child is skipped, and the status is 127 if no job is left to wait for.
  Stopped jobs don't count.

`-p var`
: Set `var` to the process ID of the job whose status is returned (the last
  process of a pipeline): with `-n`, the job that finished, and otherwise
  the job of the last argument. `var` is unset first, so it stays unset if
  no job's status is returned.

`-f`
: With job control, wait for a stopped job to end, instead of returning
  when it stops.
