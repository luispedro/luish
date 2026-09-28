# `unset`

```text
unset [-v] name... 'name[index]'... 'name[key]'...
unset -f name...
```

Remove variables or functions.

`-v`
: Remove variables (the default). Read-only variables can't be removed.

`-f`
: Remove functions.

Removing something that doesn't exist is not an error.

`unset 'a[i]'` (quoted, since `[` is a glob character) makes element `i` of
the array `a` empty, as in zsh; the array keeps its
length. `unset 'a[@]'` removes the whole array. `unset 'h[key]'` removes a
key of the associative array `h`.
