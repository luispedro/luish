# `exit`

```text
exit [n]
```

Exit the shell.

The exit status is `n`, or that of the last command run if `n` is not given.
A trap on `EXIT` runs first. In an interactive shell with stopped jobs, or
running ones (with `jobs.check_running`, on by default; see `setopt`),
`exit` only warns about them the first time; exiting again right away works,
and then sends SIGHUP to the jobs (with `jobs.hup`, also on by default).
