# `where`

```text
where [-a] [name...]
```

Show where variables were set.

`where` answers from what variable tracing recorded, so it needs
`setopt vars.trace` (or `vars.trace_history`), best turned on from the
start with `luish -o vars.trace`. For each `name`, it prints one line with
the variable's value and where it was set: the file and line, and the
function that ran, or the prompt, the `-c` command or standard input.

```sh
$ where PATH EDITOR
PATH was set to "/home/me/bin:/usr/bin" in ~/.config/luish/rc.d/path.lsh:3
EDITOR was set to "vi" in ~/.my-script.sh:123, in function setup
```

A variable can also have come from the environment (`was inherited from
the environment`), been set by luish as it started (such as `IFS` and
`PS1`), or been set before tracing began. After a function with the
variable `local` returns, or after a command with a temporary assignment
(`X=1 cmd`), `where` shows where the value put back was set. `path` is
`PATH`. Without names, `where` shows every variable that is set.

`-a`
: Show every change recorded, oldest first and numbered, with the values
  set and values put back; the last is marked `current state`. With
  `vars.trace` this is the last change only, except for `PATH`,
  `MANPATH`, `PS1`, `RPROMPT` and `RPS1`, whose last 100 changes are
  kept; `vars.trace_history` keeps the last 100 changes of every
  variable. A first line tells how many older changes were dropped, and
  a blank line separates variables.

```sh
$ where -a PATH
[1] PATH was inherited from the environment as
    "/usr/bin:/bin"
[2 - current state] PATH was set to
    "/home/me/bin:/usr/bin:/bin"
    in ~/.config/luish/rc.d/path.lsh:3
```

Values are shown in double quotes, or as `$'...'` if they have control
characters; arrays are shown as `("a" "b")`, and values longer than 4096
bytes are cut. A value too long to fit on the line (of the terminal, or
80 columns) goes on a line of its own, indented, and where it was set
on the next; with `-a`, if one of a variable's values does, they all do.
On a terminal, names, values, files and functions are in colour, in the
styles `var`, `string`, `path`, `command.function` and `comment` (see
`help style`), unless `$NO_COLOR` is set.

`where` is a built-in of interactive shells, and of other shells while
variables are traced; elsewhere `__luish_internal where` runs it. It is
not zsh's `where`, which lists every command a name finds (`type` shows
the one that runs).

The status is 0 if every variable is set or has a recorded change, 1 if
any isn't set and has none, and 2 for a bad name or if tracing is off.
