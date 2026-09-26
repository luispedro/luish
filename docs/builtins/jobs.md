# `jobs`

```text
jobs [-l | -p] [job...]
```

List background and stopped jobs.

Each job is shown with its number, its state and its command. The current
job (the default for `fg` and `bg`) is marked `+` and the previous one `-`.
Finished jobs are listed once and then forgotten.

`-l`
: Also show the process ID of each process in the job.

`-p`
: Show only the process group ID of each job.

A job is named by `%n` (its number), `%+` or `%%` (the current job), `%-`
(the previous job), `%string` (the job whose command starts with `string`)
or `%?string` (the job whose command contains `string`).
