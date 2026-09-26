# `wait`

```text
wait [job | pid...]
```

Wait for background jobs to finish.

Without arguments, `wait` waits for all background jobs, and its exit
status is 0. With arguments, it waits for each of them in turn (a process
ID, or a job as described in `help jobs`), and the exit status is that of
the last one, or 127 if it isn't a child of the shell.

A signal that has a trap interrupts the wait: the trap runs, and the exit
status is 128 plus the signal number.
