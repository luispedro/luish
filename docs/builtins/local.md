# `local`

```text
local name[=value]...
```

Make variables local to a function.

Each variable gets a new value (`value`, or, without one, the value it has)
that lasts until the function returns, when the previous value and
attributes are restored. Functions called from this one see the local
variable. Using `local` outside a function is an error.
