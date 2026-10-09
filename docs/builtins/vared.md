# `vared`

```text
vared [-Aacegh] [-p prompt] [-r rprompt] [-M keymap] [-m keymap] name
```

Edit the value of a variable with the line editor.

`vared` puts the value of `name` on a line of its own and lets you edit it
with the same keys as a command line; Enter sets the variable to the edited
text. It is zsh's `vared`:

```sh
vared PATH
vared -p 'Commit message: ' -c msg
```

The line isn't a command, so it isn't highlighted and has no
autosuggestions. Tab completes as it would in a command's argument.

An array is shown as its elements separated by the first character of
`IFS` (a space by default), with a backslash before each character of `IFS`
and each backslash in them. The edited text is split back into elements at
the characters of `IFS` that no backslash quotes; a backslash before any
other character stays. An empty element is lost, as in zsh. An
associative array is shown as its keys and values in turn, and the edited
text must have pairs of them.

`name[i]` edits an element of an array (`name[key]` one of an associative
array), which is added if there is none.

`-c`
: Create the variable if it is unset. With `-c`, it becomes a string, or
  an array with `-a`, or an associative array with `-A`, replacing a
  variable of another type.

`-a`, `-A`
: With `-c`: make the variable an array, or an associative array.

`-p prompt`
: Show `prompt` before the value, with its `%` sequences expanded as in
  `PS1` (see `setopt prompt.percent`), whether or not that option is on.

`-r rprompt`
: Show `rprompt` at the right edge, as `RPROMPT`.

`-h`
: Let the history be used (Up, Ctrl-R, ...). Without it, there is none.

`-e`
: Make Ctrl-D on an empty line give up, with status 1. Without `-e` it
  does nothing.

`-M keymap`
: Edit with `emacs` or `viins` (vi) keys, or `main`, the shell's.

`-m keymap`
: Only `vicmd`, vi's command mode, which vi editing always uses.

`-g`
: Accepted and ignored (zsh's quiets warnings luish doesn't give).

zsh's `-i` and `-f` (widgets to run first and last) and `-t` (another
terminal) are not supported.

Ctrl-C gives up as it does at the prompt: the variable is left as it was,
and the rest of the command line isn't run, unless `INT` is trapped.

`vared` is a built-in only in interactive shells (and their subshells), so
that scripts find the same commands as they do in dash.
`__luish_internal vared` is the same command in any shell whose standard
input is a terminal.

The status is 0 if the variable was set; 1 if the editing was given up, the
variable doesn't exist (without `-c`) or is read-only, there is no
terminal, or for a usage error.
