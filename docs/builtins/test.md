# `test`, `[`

```text
test expression
[ expression ]
```

Evaluate a conditional expression.

The exit status is 0 if the expression is true, 1 if it is false and 2 on
an error. With one argument, the expression is true if it is not empty.

`-e file`, `-f file`, `-d file`
: `file` exists; is a regular file; is a directory. Also `-b` (block
  device), `-c` (character device), `-h` or `-L` (symbolic link), `-p`
  (named pipe) and `-S` (socket).

`-r file`, `-w file`, `-x file`
: `file` is readable; writable; executable (or searchable).

`-s file`
: `file` exists and is not empty. Also `-u`, `-g`, `-k` (set-user-ID,
  set-group-ID and sticky bits) and `-O`, `-G` (owned by the user, group).

`-t fd`
: File descriptor `fd` is a terminal.

`file1 -nt file2`, `file1 -ot file2`, `file1 -ef file2`
: `file1` is newer; older; the same file.

`-n string`, `-z string`
: `string` is not empty; is empty.

`s1 = s2`, `s1 != s2`, `s1 < s2`, `s1 > s2`
: The strings are equal; different; sorted before; sorted after.

`n1 -eq n2`
: The integers are equal. Also `-ne`, `-lt`, `-le`, `-gt` and `-ge`.

`! expr`, `expr1 -a expr2`, `expr1 -o expr2`, `( expr )`
: Not; and; or; grouping. Parentheses must be quoted from the shell. For
  anything complicated, `test ... && test ...` is clearer.
