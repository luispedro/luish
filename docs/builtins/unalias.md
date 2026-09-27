# `unalias`

```text
unalias [-ms] name...
unalias -a [-s]
```

Remove aliases.

`-a` removes all regular and global aliases. With `-s`, the names are
suffixes, and `-as` removes all suffix aliases. With `-m`, the names are
patterns (quote them), and every alias that matches is removed. The exit
status is 1 if one of the names isn't an alias, or if no alias matches with
`-m`.
