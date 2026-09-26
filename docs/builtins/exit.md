# `exit`

```text
exit [n]
```

Exit the shell.

The exit status is `n`, or that of the last command run if `n` is not given.
A trap on `EXIT` runs first. In an interactive shell with stopped jobs,
`exit` only warns about them the first time; exiting again right away works.
