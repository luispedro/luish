# `caller`

```text
caller [n]
```

Show where the current function was called from.

As bash's `caller`, for a function or a file read with `.`.

Without `n`, print the line of the call and the file it is in (`NULL` at
the top level of a script). With `n`, print the line, the name of the
caller and its file for the `n`th frame of the call stack, counting from
0 for the innermost: the elements `n` of `BASH_LINENO` and `n+1` of
`FUNCNAME` and `BASH_SOURCE`. The caller of a function is `main` at the top
level of a script, and `source` in a file read with `.`.

The status is 1 if there is no such frame: outside a script, a function
and a file read with `.`, or for an `n` past the end of the stack.

```sh
die() {
    echo "error: $1" >&2
    caller 0 >&2    # 12 main script.sh
    exit 1
}
```
