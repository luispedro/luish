# `bg`

```text
bg [job...]
```

Continue a stopped job in the background.

The job (by default the current one) is continued as if it had been started
with `&`. This needs job control (the `monitor` option, on in interactive
shells). See `help jobs` for how to name a job.
