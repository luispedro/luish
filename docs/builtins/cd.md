# `cd`

```text
cd [-L | -P] [directory]
cd -
```

Change the working directory.

Without a directory, `cd` goes to `$HOME`; `cd -` goes back to the previous
directory (`$OLDPWD`) and prints its name. A relative directory that doesn't
start with `.` or `..` is looked for in each directory of `CDPATH`; if it is
found through `CDPATH`, the new directory is printed. `PWD` and `OLDPWD` are
set and exported.

`-L`
: Follow the path as given: `..` removes the last component of the logical
  path (with symbolic links not resolved). This is the default.

`-P`
: Use the physical directory, with symbolic links resolved.
