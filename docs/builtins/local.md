# `local`

```text
local [-aAilruUx] [+ilruUx] name[=value]... name=(value...)...
local -
```

Make variables local to a function.

Each variable gets a new value (`value`, or, without one, the value it has)
that lasts until the function returns, when the previous value and
attributes are restored. Functions called from this one see the local
variable. Using `local` outside a function is an error.

`local -` makes the options that `set` sets (as shown by `$-` and `set -o`)
local: they are restored when the function returns, as in dash. luish's own
options, set with `setopt`, are not restored.

`local name=(a b c)` makes a local array. The options (not POSIX) set
attributes, as for `typeset`, which also makes local variables, but starts
them unset.
