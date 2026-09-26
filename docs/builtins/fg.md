# `fg`

```text
fg [job...]
```

Continue a job in the foreground.

The job (by default the current one) gets the terminal, is continued if it
was stopped, and the shell waits for it. The exit status is that of the job.
This needs job control (the `monitor` option, on in interactive shells). See
`help jobs` for how to name a job.
