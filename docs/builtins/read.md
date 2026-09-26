# `read`

```text
read [-r] [-p prompt] name...
```

Read a line from standard input into variables.

The line is split into fields with `IFS`, as in field splitting: each name
gets one field, and the last one gets the rest of the line (without leading
and trailing `IFS` whitespace). Names left over are set to empty. The exit
status is 1 at the end of the input (the variables are still set from what
was read).

Unless `-r` is given, a backslash quotes the next character, and a
backslash at the end of the line continues it on the next line.

`-r`
: Treat backslashes as ordinary characters.

`-p prompt`
: Print `prompt` on standard error first, if standard input is a terminal.

```sh
while IFS=: read -r user _ uid _; do
    echo "$user has uid $uid"
done < /etc/passwd
```
