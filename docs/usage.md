# Usage

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`.

## Saving and restoring the shell's state

luish's own commands are subcommands of the `__luish_internal` built-in. `__luish_internal savestate` prints shell
commands that recreate the current state of the shell: the working directory, the file mode mask (`umask`),
variables (with their `export` and `readonly` attributes), traps, functions, aliases and options. Reading them back with `.` restores that state, in the same shell or in another:

```sh
__luish_internal savestate > ~/saved.sh
luish -c '. ~/saved.sh; myfunction'
```

Restoring adds to the current state: variables, functions and aliases defined since are kept. A variable that is
already `readonly` can't be restored, so reading the state back into the shell that saved it fails if it has any.
In `eval "$(__luish_internal savestate)"`, traps are lost, because a command substitution resets them.

## The version of luish

`__luish_internal print-git-rev` prints the git revision luish was built from, and `__luish_internal
print-git-rev-short` the same with the abbreviated hash. A build from sources with uncommitted changes adds `-dirty`,
and a build outside a git checkout prints `unknown`.

## Cached startup files

luish can cache the effects of your startup files, so that a new shell restores their result instead of running
them, which is much faster when they run slow commands. The files go in two directories, each used only if it exists
(they work like zsh's `.zshrc` and `.zlogin`):

- `~/.config/luish/rc.d/` (or `$XDG_CONFIG_HOME/luish/rc.d/`): for every interactive shell. This is the place for
  what isn't inherited by the shells you start: aliases, functions and options. Scripts and `luish -c` don't read it.
- `~/.config/luish/login.d/`: for login shells, after `rc.d`. Its files replace `/etc/profile` and `~/.profile`. This
  is the place for the environment: exported variables such as `PATH`, which the programs and shells you start
  inherit.

In each directory, the files whose names end in `.lsh` run in byte order. luish remembers what they did: the variables
they set, exported or unset, and their functions, aliases, options, traps and `umask`. An interactive shell then
reads `$ENV` and `luishrc`, uncached, as usual.

```sh
mkdir -p ~/.config/luish/login.d ~/.config/luish/rc.d
echo '. /etc/profile' > ~/.config/luish/login.d/00-system.lsh
echo 'export PATH=$HOME/bin:$PATH EDITOR=vim' > ~/.config/luish/login.d/10-env.lsh
echo "alias ll='ls -l'" > ~/.config/luish/rc.d/aliases.lsh
```

The caches are `~/.cache/luish/rc-HOST` and `~/.cache/luish/login-HOST` (or under `$XDG_CACHE_HOME`). A cache is
used as long as the `.lsh` files and the files they read with `.` are unchanged, and luish itself is the same build.
When one of them changes (luish compares their size and modification time), or after luish is upgraded, the next
shell reruns the files and updates the cache.

Some things belong in a directory's `_uncached.lsh`, which runs every time, after that directory's cached state is
restored:

- anything with side effects, such as starting `ssh-agent` or printing a message;
- values that differ between shells, such as `GPG_TTY=$(tty)`;
- anything that depends on the environment the shell was started in, such as `$SSH_CONNECTION` or `$DISPLAY`. The
  cached values are those from when the cache was built, so, for example, `PATH=$HOME/bin:$PATH` keeps the rest of
  the `PATH` that the shell had then. For the same reason, a file in `rc.d` shouldn't use variables set in
  `login.d` (in a login shell, `rc.d` runs before them; in the shells started from it, they are inherited).

Changes that luish can't see, such as newly installed software, the output of commands, or files tested with `[`,
don't refresh the caches: remove the cache files (or touch a `.lsh` file) after such changes.
