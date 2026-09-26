# `fc`

```text
fc [-e editor] [-nlr] [first [last]]
fc -s [old=new] [first]
```

List, or edit and run again, commands from the history.

`first` and `last` select a range of the history: a positive number is an
event number, a negative one counts back from the latest command, and
anything else selects the latest command that starts with it. `fc` itself
isn't in the history it works on.

By default, the commands are written to a file and an editor is opened on
it (`$FCEDIT`, else `$EDITOR`, else `ed`); when the editor exits, the file
is run, and replaces the `fc` command in the history.

`-e editor`
: Use `editor`. `-e -` runs the commands without editing them, as `-s`.

`-l`
: List the commands (by default the last 16) instead.

`-n`
: With `-l`, leave out the event numbers.

`-r`
: Reverse the order of the commands.

`-s`
: Run the command again (by default the latest) without editing it; with
  `old=new`, the first `old` in it is replaced by `new` first.

`fc` only works in interactive shells.
