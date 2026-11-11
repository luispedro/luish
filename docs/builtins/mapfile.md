# `mapfile`

```text
mapfile [-t] [-d delim] [-n count] [-O origin] [-s count] [-u fd]
        [-C callback] [-c quantum] [array]
readarray ...
```

Read the lines of standard input into an array.

Not POSIX; as in bash. Each line becomes one element of `array` (`MAPFILE`
without a name), which is emptied first unless `-O` is given. A line keeps
its newline unless `-t` is given, and a last line without a newline counts
too. Unlike `read`, nothing is split or trimmed, and a backslash is an
ordinary character. `readarray` is another name for it.

`-t`
: Remove the newline (or the delimiter of `-d`) from the end of each line.

`-d delim`
: End each line at the first character of `delim` instead of at a newline.
  `-d ''` ends them at a NUL byte, as in
  `find . -print0 | mapfile -t -d '' files`. Otherwise NUL bytes are dropped.

`-n count`
: Read at most `count` lines (all of them if `count` is 0). The rest of the
  input is left to be read.

`-s count`
: Skip the first `count` lines.

`-O origin`
: Assign the lines from index `origin` on, keeping the other elements of
  the array. Elements missing before `origin` are made empty.

`-u fd`
: Read from file descriptor `fd` instead of standard input.

`-C callback`
: Evaluate `callback` each time `quantum` lines have been read, with the
  index of the element about to be assigned and its line as two further
  arguments.

`-c quantum`
: The number of lines between two calls of `callback` (5000 without it).

The exit status is 0 unless an option or the name is invalid, or the array
is read-only or an associative one.

Run it in the shell itself for the array to stay: in `cmd | mapfile lines`
it is set in a subshell and lost. Use a redirection, such as a
here-document or a process substitution:

```sh
mapfile -t lines < file
mapfile -t tags < <(git tag)
mapfile -t -d '' files < <(find . -name '*.c' -print0)
echo "${#lines[@]} lines, the first is ${lines[0]}"
```
