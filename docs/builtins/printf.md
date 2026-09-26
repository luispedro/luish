# `printf`

```text
printf format [argument...]
```

Print the arguments according to a format.

The format is printed with backslash escapes (as in `echo`) interpreted and
each conversion replaced by the next argument. If arguments remain, the
format is used again. Missing arguments count as empty strings or zero.

`%s` `%c`
: A string; its first character.

`%d` `%i` `%u` `%o` `%x` `%X`
: An integer, in decimal, octal or hexadecimal. An argument that starts with
  a quote stands for the value of the character after it.

`%f` `%e` `%g` `%E` `%G` `%a` `%A`
: A floating-point number.

`%b`
: A string, with backslash escapes in it interpreted, as `echo` does.

`%%`
: A `%` sign.

Flags (`-`, `+`, space, `#`, `0`), a width and a precision may come after
the `%`, as in C; `*` takes them from the arguments.

```sh
printf '%-10s|%5.2f\n' name 3.14159    # name      | 3.14
```
