# `pushd`, `popd`, `dirs`

```text
pushd [-qLP] [directory | +n | -n]
popd [-qLP] [+n | -n]
dirs [-lpv]
dirs -c
dirs directory ...
```

Keep a stack of directories to go back to.

These commands are not in POSIX; they work as in zsh. The stack is listed
with the current directory first, as entry 0, then the directories that
were pushed, most recent first. `+n` names entry `n` from the start of this
list, and `-n` entry `n` from its end (`-0` is the last one).

`pushd directory` changes to the directory as `cd` does (`pushd -` goes to
`$OLDPWD`) and pushes the previous one onto the stack. Without a directory,
it swaps the top two entries (it goes to `$HOME` if the stack is empty).
`pushd +n` or `-n` goes to that entry and rotates the stack, so that the
entries before it move to the end.

`popd` removes the top of the stack and goes to it; the entry is removed
even if the directory can't be changed to. `popd +n` or `-n` removes that
entry instead, without changing directory.

The array `dirstack` holds the stack without the current directory, so
that `${dirstack[0]}` is entry 1; assigning an array to it, as in
`dirstack=(~/src /tmp)`, replaces the stack.

In an interactive shell, `pushd` and `popd` print the stack afterwards, as
`dirs` does, unless `pushd.silent` is set.

Three of zsh's options (see `setopt`) change how the stack is kept:
`pushd.auto` (zsh's `auto_pushd`) makes `cd` push the previous directory,
as `pushd` does, and lets it take `+n` and `-n` to take that entry out of
the stack and go to it. `pushd.ignore_dups` keeps one copy of each
directory: after `cd`, `pushd` or `popd`, the new directory is removed
from the stack. `pushd.silent` stops `pushd` and `popd` from printing the
stack.

`-q`
: For `pushd` and `popd`: don't print the stack.

`-L`, `-P`
: For `pushd` and `popd`: as for `cd`, except that `-P` wins over `-L`
  whatever their order.

`dirs` prints the stack on one line, with `$HOME` shown as `~`.

`-l`
: Print full names, without `~`.

`-p`
: Print one directory per line.

`-v`
: Print one directory per line, with its number.

`-c`
: Clear the stack.

`dirs directory ...` replaces the stack (after the current directory) with
the directories given.

The exit status is 0 on success, and 1 if the directory can't be changed or
the entry doesn't exist.
