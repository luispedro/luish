# `type`

```text
type name...
```

Describe how each name would be run as a command.

Each name is shown as a keyword, an alias, a special built-in, a function, a
built-in, or the path of a program (a "tracked alias" if the shell has
remembered its path). The exit status is 127 if one of them isn't found.
