# Remote shells over SSH

Over ssh, every key you type goes to the server and back before it shows. On a
slow connection, typing lags, the cursor jumps, and a completion menu redraws
one round trip at a time.

`luish --ssh HOST` splits the shell in two. The line editor runs on your
machine, and the shell that runs your commands runs on HOST. Typing, moving in
the line, searching the history, autosuggestions and the completion menu are
instant, however slow the connection. Commands, Tab completion, the prompt and
everything your startup files set up on HOST still come from HOST.

```sh
luish --ssh myserver                  # the first time, copies luish to myserver
luish --ssh -p 2222 me@myserver       # ssh's options go before the host
```

luish uses ssh as it is, with your keys, your agent and `~/.ssh/config`. It
needs no daemon on HOST and no extra ports, and luish needn't be installed
there: `luish --ssh` copies itself (see [below](#luish-on-the-server)).

Only your machine needs a terminal; both ends must run Linux.

## What runs where

| On your machine | On HOST |
|---|---|
| Typing and moving in the line, key bindings | Commands, and their jobs |
| The history's up-arrow, searches and autosuggestions | The history file, `fc` and `!!` |
| Drawing the completion menu | Finding what Tab completes (with HOST's plugins and completers) |
| Syntax highlighting | The files and commands that highlighting looks up |
| | The prompt, startup files, `config.toml` and plugins |

So the session is configured by HOST's `config.toml`, `rc.d` and plugins, not
by yours: its prompt, colours, key bindings (emacs or vi), aliases and
completion are HOST's. Your machine only lends the editor.

Each command line costs one round trip, when you press Enter, and so does each
Tab. Highlighting asks HOST whether the words that look like paths are files,
and waits for the answer at most 20 ms; an answer that comes later shows the
next time the line is redrawn (at the next key). Commands are highlighted
without waiting: HOST sends the names of the commands on its `PATH` with each
prompt (only when they change).

## While a command runs

Between command lines, your terminal is connected to a terminal (a pty) on
HOST, as with `ssh -t`: keys go to the command as you type them, and its output
comes back as it is printed. So full-screen programs (vim, less, top, tmux)
work as they do over ssh, and so do job control, `^C`, `^Z` and window size
changes.

Keys you type before the next prompt shows are not lost, nor read by the
command that just finished: they start the next line. A line typed ahead with
its Enter runs as soon as the prompt comes.

The output of background jobs shows as it comes, also while you edit a line.

## Escapes

ssh's escapes work as they do in ssh: the escape character, `~`, at the start
of a line, then a key.

| Escape | What it does |
|---|---|
| `~.` | Closes the connection (even one that hangs), and luish exits with status 255, as ssh does |
| `~^Z` | Suspends luish, back to the shell you started it from (`fg` continues it). ssh keeps the connection open meanwhile |
| `~?` | Lists the escapes |
| `~~` | Types a single `~`, so `~~.` closes an ssh that runs on HOST |

While a command runs, "the start of a line" is after Enter, as in ssh. At
the prompt, it is an empty line: `~` goes into the line as any other key, and
the key after it makes the escape. Any other key after `~` is typed as usual
(so `~/bin` and `~user` need nothing special), and so are ssh's other escapes
(`~C`, `~#`, `~R`, `~B`, `~V`, `~v` and `~&`), which need ssh itself.
On a terminal where luish doesn't edit lines (`TERM=dumb`), type the escape
as a line of its own, ended with Enter.

`-o ssh.escape_char=C` makes another printable character the escape
character, and `-o ssh.escape_char=none` turns the escapes off:

```sh
luish --ssh -o ssh.escape_char=% myserver
```

ssh's own escape character (its `-e`, or `EscapeChar` in `~/.ssh/config`)
does nothing here: ssh never sees your keys, only luish's messages.

## The shell on HOST

`luish --ssh` starts an interactive shell on HOST, but not a login shell (plain
`ssh HOST` starts a login shell). So it reads `config.toml`, `rc.d`, `$ENV`
and `luishrc` on HOST, but not `login.d`, `/etc/profile` or `~/.profile`.
Settings that only a login shell makes, such as a `PATH` set in `~/.profile`,
are missing: set them in `config.toml` instead (its `[env]` and `[path]`
tables apply to every interactive shell, see
[](usage.md#settings-in-configtoml)), or start a login shell with `--remote`
(see [below](#other-transports)).

The variables that describe your terminal (`TERM`, `COLORTERM`, `TERM_PROGRAM`
and `TERM_PROGRAM_VERSION`) are passed to HOST, replacing its own (as ssh does
with `TERM`), so that programs there know what your terminal can do. So is the
locale (`LANG`, `LANGUAGE` and `LC_*`), but only where HOST hasn't set it,
because HOST may not have your locale.

The session ends when the shell on HOST exits (with `exit`, `^D`, or a command
it `exec`ed), and `luish --ssh` exits with its status. If the connection is
lost, luish says so and exits with status 255, as ssh does.

## luish on the server

The first time you connect to a host, `luish --ssh` copies itself there, in the
same ssh connection (so you log in once):

```console
$ luish --ssh myserver
luish: copying luish 0.5.0 to the server (5.6 MB)
```

The copy is kept on HOST as `~/.cache/luish/binaries/luish-VERSION-BUILD`
(BUILD being the git commit it was built from), or under `$XDG_CACHE_HOME` if
the commands that ssh runs have it set. Later connections run that copy. Each
version (and build) of luish thus runs its own copy, whatever else is
installed on HOST, so both ends always speak the same protocol. When a new copy
is made, the copies more than 30 days old are removed.

The copy isn't made, and the `luish` on HOST's `PATH` is run instead, when:

- HOST is another system or architecture (as `uname -sm` says), such as an
  `aarch64` server for an `x86_64` laptop;
- the copy can't run there, as with a build linked with a newer C library than
  HOST's. The release builds run on any Linux from 2014 on (see
  [](installation.md)).

luish says why on standard error. The `PATH` that ssh gives commands is often
shorter than a login shell's (without `~/.local/bin`, for example), so if
luish is installed somewhere else, name it with `--luish-path`:

```sh
luish --ssh --luish-path='~/.local/bin/luish' myserver
luish --ssh --luish-path=luish myserver        # the luish on the PATH, never a copy
```

`--luish-path=PROGRAM` (as rsync's `--rsync-path`) runs PROGRAM on HOST and
copies nothing. HOST's own shell runs it, so it may start with `~/`. PROGRAM
must be the same version of luish as yours.

Two options control the copy (`-o ssh.escape_char` is [above](#escapes)):

```sh
luish --ssh -o ssh.no_auto_copy myserver   # never copy luish
luish --ssh --copy-luish myserver          # copy luish, even if HOST has the copy
```

`-o ssh.no_auto_copy` runs the copy if HOST has it, and otherwise the `luish`
on HOST's `PATH`, without copying anything (as on a slow or metered link).
Its name is matched as `setopt`'s: case and `_` don't matter
(`-o ssh.NoAutoCopy`), and `-o ssh.auto_copy` turns it back off.
`--copy-luish` copies luish to HOST again, replacing the copy there (if it was
damaged, say), and overrides `-o ssh.no_auto_copy`.

## Other transports

`luish --ssh [OPTION...] HOST` is a short way to write

```sh
luish --remote ssh -T [OPTION...] HOST luish --serve
```

plus the copy.

To reach HOST with another command than `ssh -T`, name it with `-e COMMAND`
(or `--rsh=COMMAND`, as in rsync, or `--ssh-command=COMMAND`, as git's
`core.sshCommand`). COMMAND is split into words at blanks, with
quotes as in the shell, and is run with ssh's options (the rest of the
`--ssh` arguments) and HOST after it, then the command for HOST; it replaces
`ssh -T` as a whole:

```sh
luish --ssh -e 'ssh -T -F ~/.ssh/work-config' myserver
luish --ssh -e 'docker exec -i' mycontainer    # anything that runs a command
```

ssh's own `-e` (its escape character) isn't available, as it would do
nothing: ssh's input is a pipe to luish, not your terminal. luish has the
escapes instead ([above](#escapes)).

`luish --remote COMMAND...` runs COMMAND with its standard input
and output connected to luish, and COMMAND must start `luish --serve` at the
other end. `luish --serve` takes the usual options after `--serve`, such as
`-l` for a login shell:

```sh
luish --remote ssh -T myserver luish --serve -l        # a login shell on myserver
luish --remote luish --serve                           # both ends here, to try it out
```

With `--remote`, nothing is copied: the `luish` at the other end must be the
same version as yours, or the client stops with

```text
luish: the server's luish (0.4.0) speaks another version of the protocol than this one (0.5.0)
```

## Making it the way you log in

An alias in `config.toml` saves typing the option:

```toml
[alias]
myserver = "luish --ssh myserver"
```

or, for every host, a function in `rc.d`:

```sh
sshl() { luish --ssh "$@"; }
```

## Messages

What HOST's startup files print before luish starts (such as a message of the
day from `~/.bashrc`) is shown on standard error. luish's own messages are:

| Message | Meaning |
|---|---|
| `copying luish VERSION to the server (N MB)` | The first connection to HOST with this build; later ones reuse the copy |
| `the server runs SYSTEM, not SYSTEM: using the server's own luish` | HOST is another system or architecture, so the `luish` on its `PATH` runs |
| `the copy of luish cannot run on this host` | The copy didn't run on HOST (an older C library), so the `luish` on its `PATH` runs |
| `cannot copy luish to FILE` | The copy couldn't be written on HOST (a full or read-only disk) |
| `the server's luish (VERSION) speaks another version of the protocol than this one (VERSION)` | With `--luish-path` or `--remote`, the two ends are different versions |
| `the connection to the server was lost` | ssh exited, or the network went away |
| `the connection to the server was closed` | You typed `~.` |
| `--copy-luish and --luish-path can't be used together` | `--luish-path` names the luish to run, so nothing is copied |
| `--ssh: unknown option ssh.NAME` | Only `-o ssh.no_auto_copy` (and `-o ssh.auto_copy`) and `-o ssh.escape_char` are luish's |
| `--ssh: ssh.escape_char must be a printable character or none` | `-o ssh.escape_char` takes one character, such as `%`, not ssh's `^]` |
| `--remote: standard input and output must be a terminal` | `luish --ssh` is for interactive use; for scripts, use `ssh HOST luish -c '...'` |

## Limitations

- **A dropped connection ends the session**, as with ssh: unlike mosh, luish
  doesn't reconnect. Run tmux or screen on HOST to keep work going across
  disconnections.
- **`^C` doesn't interrupt a Tab** that waits for HOST, and `~.` doesn't
  close the connection then: the completion comes when HOST answers.
- **Output while you edit**: what background jobs print while you edit a line
  is written as it comes, and the line isn't redrawn below it.
- **Highlighting of paths may lag** on a slow connection: a late answer shows
  at the next key, not as soon as it comes.
- **Both ends must run the same version.** `--ssh` makes sure of it unless it
  is given `--luish-path`; a mismatch is an error, not a fallback.
- **Linux only**, on both ends, and the copy needs the same system and
  architecture (`uname -sm`) on both.
