# `read`

```text
read [-ers] [-a array] [-d delim] [-i text] [-n count] [-N count]
     [-p prompt] [-t timeout] [-u fd] [name...]
read [options] -A [name]
```

Read a line from standard input into variables.

The line is split into fields with `IFS`, as in field splitting: each name
gets one field, and the last one gets the rest of the line (without leading
and trailing `IFS` whitespace). Names left over are set to empty. Without a
name, the whole line goes to `REPLY`, as it is (not POSIX; as in bash). The
exit status is 1 at the end of the input (the variables are still set from
what was read).

Unless `-r` is given, a backslash quotes the next character, and a
backslash at the end of the line continues it on the next line.

POSIX has only `-r` and `-p`, and `-p` only as in bash (zsh's `-p` reads
from a coprocess). The other options are bash's, and `-A` is zsh's.

`-r`
: Treat backslashes as ordinary characters.

`-p prompt`
: Print `prompt` on standard error first, if the input is a terminal.

`-d delim`
: End the line at the first character of `delim` instead of at a newline.
  `-d ''` ends it at a NUL byte, as in
  `find . -print0 | while IFS= read -r -d '' f; do ...; done`. Otherwise
  NUL bytes are dropped.

`-n count`
: Return after reading `count` characters, or at the delimiter if it comes
  first. On a terminal, the characters needn't be followed by Enter.

`-N count`
: Read exactly `count` characters, unless the input ends first. The
  delimiter is an ordinary character, and the text isn't split: the first
  name gets all of it.

`-t timeout`
: Give up after `timeout` seconds, which may have a fraction (`0.5`), with
  exit status 142. The variables get what was read until then. With `-t 0`,
  nothing is read: the exit status is 0 if there is input to read (or the
  input has ended), and 1 if not.

`-u fd`
: Read from file descriptor `fd` instead of standard input.

`-s`
: Don't echo the characters typed, if the input is a terminal (for
  passwords).

`-e`
: Read the line with the line editor, if standard input is a terminal (and
  without `-s`, `-n`, `-N`, `-t` and `-d`). Ctrl-D on an empty line is the
  end of the input. `-E` is the same.

`-i text`
: With `-e`, start with `text` in the editor.

`-a name`
: Read the fields into the array `name`. Further names are ignored. The
  line is split as for several names, but the last field ends at the next
  delimiter too; an empty line gives an empty array.

`-A`
: The same as `-a`, with the array as the one name (`reply` if there is
  none), as in zsh. An associative array gets pairs of keys and values.

```sh
while IFS=: read -r user _ uid _; do
    echo "$user has uid $uid"
done < /etc/passwd

read -rsp 'Password: ' pw; echo
read -rn 1 -p 'Continue? [y/n] ' answer; echo
read -e -i "$HOME/" -p 'Directory: ' dir
```
