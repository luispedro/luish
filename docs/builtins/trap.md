# `trap`

```text
trap [action] signal...
trap - signal...
trap
```

Run a command when the shell receives a signal or exits.

`action` is run (as with `eval`) when one of the signals arrives. An empty
`action` ignores the signals, and `-` restores their default behaviour (as
does leaving out the action, when there is one signal or the first one is a
number). Signals are named without `SIG`
(`INT`, `TERM`, `HUP`...) or by number; `EXIT` or `0` is the shell exiting.
Without arguments, `trap` lists the traps as commands that can be read back.

Traps are reset in subshells, except ignored signals. Signals that were
ignored when a shell that isn't interactive started can't be trapped.

```sh
tmp=$(mktemp) && trap 'rm -f "$tmp"' EXIT
```
