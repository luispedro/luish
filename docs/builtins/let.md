# `let`

```text
let expression...
```

Evaluate arithmetic expressions.

`let` evaluates each argument as an arithmetic expression, as in `$((...))`,
so `let x=1+2 'y = x * 3'` sets `x` and `y`. It is not in POSIX, and follows
zsh. Its status is 0 if the value of the last expression is non-zero and 1 if
it is zero, so `let 'n > 0'` works as a test. An invalid expression prints an
error, stops at that argument and gives status 1, without exiting the shell.
Arguments are expanded like those of any command, so quote those with `*`,
spaces or other characters special to the shell. Unlike zsh, arithmetic is
on integers only.
