# `:`

```text
: [argument...]
```

Do nothing, successfully.

The arguments are expanded, so `:` is useful for expansions that have side
effects, such as `: ${VAR:=default}`. The exit status is 0. `:` is a special
built-in: assignments before it stay set.
