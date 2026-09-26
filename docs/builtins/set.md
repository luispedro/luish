# `set`

```text
set [-abCefhImnuvx] [-o option]... [argument...]
set [+abCefhImnuvx] [+o option]... [argument...]
set -- [argument...]
set -o | +o
```

Set options or positional parameters.

Without arguments, `set` lists all variables. `-x` turns an option on and
`+x` turns it off, and `-o name` and `+o name` do the same by name. `set -o`
shows the options, and `set +o` shows them as commands that restore them.
Arguments after the options replace the positional parameters (`$1`, `$2`,
...); `set --` with no arguments after it removes them all.

`-a` allexport
: Export every variable that is assigned.

`-b` notify
: Report finished background jobs at once. Accepted, but not implemented
  yet: they are reported before the next prompt.

`-C` noclobber
: `>` doesn't overwrite existing files (`>|` still does).

`-e` errexit
: Exit when a command fails, except in conditions (`if`, `while`, the left
  of `&&` and `||`, after `!`).

`-f` noglob
: Don't expand filename patterns.

`-h` hashall
: Accepted, as POSIX requires; luish remembers where every command is found
  anyway.

`-I` ignoreeof
: Don't exit an interactive shell on end of file (Ctrl-D).

`-m` monitor
: Job control: run each job in its own process group. On in interactive
  shells.

`-n` noexec
: Read commands without running them (ignored in interactive shells).

`-u` nounset
: Expanding an unset variable is an error.

`-v` verbose
: Print input lines as they are read.

`-x` xtrace
: Print each command, after expansion, before it runs (prefixed by `$PS4`).

`-E` emacs, `-V` vi
: Line editing mode of an interactive shell.

luish's own options, beyond these, are set with `setopt` and `unsetopt`,
which also take the names above.
