# `echo`

```text
echo [-n] [argument...]
```

Print the arguments, separated by spaces.

A newline follows, unless the first argument is `-n` (no other options are
recognised). Backslash escapes in the arguments are always interpreted:

`\a` `\b` `\f` `\n` `\r` `\t` `\v`
: Alert, backspace, form feed, newline, carriage return, tab, vertical tab.

`\e`
: Escape.

`\\`
: A backslash.

`\0nnn`, `\nnn`
: The byte with the octal value `nnn` (up to three digits).

`\c`
: Print nothing more, not even the newline.

For portable output, `printf` is a better choice.
