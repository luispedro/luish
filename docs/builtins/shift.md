# `shift`

```text
shift [n]
```

Shift the positional parameters to the left.

`$2` becomes `$1`, `$3` becomes `$2`, and so on; `n` (by default 1) of them
are removed. Shifting more than there are is an error.
