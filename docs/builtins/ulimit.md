# `ulimit`

```text
ulimit [-H | -S] [-a | -tfdscmlpnvwr] [limit]
```

Show or set limits on resources (such as memory or files).

With a `limit` (a number, or `unlimited`), set it; otherwise show it. By
default, a new limit sets both the soft limit and the hard limit, and the
soft limit is shown.

`-H`, `-S`
: Only the hard limit (which can't be raised again, except by root); only
  the soft limit.

`-a`
: Show all the limits.

`-f`
: File size, in blocks of 512 bytes (the default).

`-t`
: CPU time, in seconds.

`-d`, `-s`, `-m`, `-l`, `-v`
: Data segment, stack, resident memory, locked memory and address space,
  in kilobytes.

`-c`
: Core file size, in blocks of 512 bytes.

`-n`, `-p`, `-w`, `-r`
: Open files; processes; file locks; real-time priority.
