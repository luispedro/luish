# `cd`

```text
cd [-L | -P [-e]] [directory]
cd -
chdir ...
```

Change the working directory.

Without a directory, `cd` goes to `$HOME`; `cd -` goes back to the previous
directory (`$OLDPWD`) and prints its name. A relative directory that doesn't
start with `.` or `..` is looked for in each directory of `CDPATH`; if it is
found through `CDPATH`, the new directory is printed. `PWD` and `OLDPWD` are
set and exported. `chdir` is another name for `cd`.

With `setopt auto_pushd`, as in zsh, `cd` also pushes the previous
directory onto the directory stack, and `cd +n` or `cd -n` takes that entry
out of the stack and goes to it (see `pushd`).

`-L`
: Follow the path as given: `..` removes the last component of the logical
  path (with symbolic links not resolved). This is the default.

`-P`
: Use the physical directory, with symbolic links resolved.

`-e`
: With `-P`, return 1 if the directory was changed but its name can't be
  found (for example, because it was removed). `PWD` is then set to the
  logical path. Without `-e`, `cd` returns 0 in that case.

The exit status is 0 on success, 1 in the case of `-e` above, and 2 if the
directory can't be changed.
