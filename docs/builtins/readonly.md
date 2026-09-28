# `readonly`

```text
readonly name[=value]... name=(value...)...
readonly -p
```

Make variables read-only.

Each variable is set to `value`, if one is given, and can't be assigned or
unset afterwards (in this shell and its subshells). Without names, or with
`-p`, `readonly` lists the read-only variables as commands that can be read
back. `readonly name=(a b c)` makes a read-only array,
whose elements can't be assigned either.
