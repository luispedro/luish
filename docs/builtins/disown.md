# `disown`

```text
disown [-ahr] [job...]
```

Remove jobs from the job table.

A job that is disowned keeps running, but the shell forgets it: `jobs`
doesn't list it, its end isn't reported, `wait` without arguments doesn't
wait for it, and `exit` doesn't warn about it if it is stopped. Without
arguments, `disown` removes the current job, so `command & disown` starts a
command that the shell then leaves alone; zsh's `command &|` and `command &!`
do the same. See `help jobs` for how to name a job; as in bash, a job can
also be named by the process id of one of its processes, such as `$!`.

A stopped job stays stopped once disowned, with a warning that says how to
continue it (`kill -CONT`).

This command is not in POSIX; it works as in zsh, with bash's options:

`-a`
: Remove all jobs.

`-r`
: Remove only running jobs: all of them, if no job is named.

`-h`
: Leave the jobs in the table. In bash, this keeps the shell from sending
  them SIGHUP when it exits; luish never does that, so `-h` does nothing.

The exit status is 1 if there is no current job, or a job named doesn't
exist (the others are still removed), and 0 otherwise.
