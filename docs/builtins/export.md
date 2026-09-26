# `export`

```text
export name[=value]...
export -p
```

Pass variables to the commands the shell runs.

Each variable is marked for export, and set to `value` if one is given.
Arguments that look like assignments are expanded as assignments are (no
field splitting or globbing), so `export PATH=$PATH:~/bin` needs no quotes.
Without names, or with `-p`, `export` lists the exported variables as
commands that can be read back.
