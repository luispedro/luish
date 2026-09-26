# `umask`

```text
umask [-S] [mode]
```

Show or set the file mode creation mask.

The permissions in the mask are removed from the files and directories that
the shell and its children create. `mode` is octal (`umask 022`) or
symbolic (`umask u=rwx,g=rx,o=`, which names the permissions to allow).
Without `mode`, the mask is shown in octal.

`-S`
: Show the mask in symbolic form (`u=rwx,g=rx,o=rx`).
