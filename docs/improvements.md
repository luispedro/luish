# Improvements over other shells

luish is a POSIX shell, and in scripts it behaves like dash. In interactive use, though, it fixes some long-standing
annoyances of the traditional shells. This page lists them.

## Newly installed commands are found

Shells remember where they found each command in `PATH`, so they don't have to search every directory each time
you run it. The `hash` built-in shows that list. In dash, bash and zsh, the list stays as it is until you change
`PATH` or run `hash -r` (`rehash` in zsh), even when the directories change on disk. Some cases that go wrong:

- **A new command shadows an old one.** You ran `python` (found in `/usr/bin`), then installed another one in
  `~/.local/bin`, which comes earlier in your `PATH`. The shell keeps running `/usr/bin/python`.
- **A command moves.** In bash, running a command that has since moved or been deleted fails with
  `bash: /usr/local/bin/tool: No such file or directory`, even if it is still in another `PATH` directory.
- **A profile is switched.** Nix puts a directory in `PATH` that is really a symlink to the current version of your
  profile. Switching profiles can add a command that shadows a remembered one, as above. Watching modification
  times doesn't catch this either, because every directory in the Nix store has the same one (1 January 1970).

luish checks the `PATH` directories after each command line you enter. If any of them changed (a file was added,
removed or renamed in it, or it is now a different directory), luish discards the whole list, so the next search
finds the right command. You never need `hash -r`.

```console
$ PATH=~/a:~/b:$PATH
$ tool                  # only in ~/b
from b
$ cp tool-v2 ~/a/tool   # (or installed from another terminal)
$ tool
from a
```

If a remembered command has been deleted, luish looks for it in the rest of `PATH` instead of failing, as dash does
(bash doesn't).

**Cost.** The check is one `stat` per `PATH` directory: about 20 system calls, which take a few microseconds in
total. It runs only in interactive shells, so scripts are unaffected. luish doesn't use inotify (which Linux limits
to 128 instances per user by default, shared with editors and other programs), and needs no background activity.

**Limits.** A change is noticed on the next command line. A command installed and then run on the same line (such
as `pip install --user tool; tool`) is still found by a normal search if it wasn't cached before; only a command
that shadows a cached one is missed until the next line.

The same check keeps Tab completion of command names up to date: new commands in `PATH` are offered as soon as they
are installed.
