# `eval`

```text
eval [argument...]
```

Run the arguments as a command.

The arguments are joined with spaces, and the result is parsed and run in
the current shell. The exit status is that of the command, or 0 if there is
none.

```sh
var=HOME
eval "echo \$$var"      # prints the value of $HOME
```
