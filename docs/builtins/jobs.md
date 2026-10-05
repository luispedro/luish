# `jobs`

```text
jobs [-l | -p] [job...]
jobs -i [job]
```

List background and stopped jobs.

Each job is shown with its number, its state and its command. The current
job (the default for `fg` and `bg`) is marked `+` and the previous one `-`.
Finished jobs are listed once and then forgotten.

`-l`
: Also show the process ID of each process in the job.

`-p`
: Show only the process group ID of each job.

`-i`
: Show a menu of the jobs, on the terminal, to act on them (see below), with
  `job` selected (by default the current job).

The menu lists every job with its number, process ID, state and command, and
follows them as they change. Up and Down (or `j` and `k`), or a job's number,
choose a job, and these keys act on it:

`f` or Enter
: Bring it to the foreground, as `fg` does (this leaves the menu).

`b`
: Continue it in the background, as `bg` does.

`s`
: Stop it (with `STOP`).

`K`
: List the signals that end it. The next key sends one: `t` for `TERM`, `e`
  for `TERM` and then `KILL` if the job hasn't ended 5 seconds later, `k` for
  `KILL`, `i` for `INT` and `h` for `HUP`. Any other key sends nothing, so a
  job is never ended by one key. A stopped job is continued after the signal,
  so that it can act on it.

`q`, Esc or Ctrl-C leave the menu. If `KILL` is still to be sent, leaving waits
for it (Ctrl-C again leaves without sending it). Jobs that stopped or ended are
reported before the next prompt, as usual.

A job is named by `%n` (its number), `%+` or `%%` (the current job), `%-`
(the previous job), `%string` (the job whose command starts with `string`)
or `%?string` (the job whose command contains `string`).
