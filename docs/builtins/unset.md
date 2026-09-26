# `unset`

```text
unset [-v] name...
unset -f name...
```

Remove variables or functions.

`-v`
: Remove variables (the default). Read-only variables can't be removed.

`-f`
: Remove functions.

Removing something that doesn't exist is not an error.
