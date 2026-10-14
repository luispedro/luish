# `shopt`

```text
shopt [-pqsu] [-o] [name...]
```

Set or show bash's options.

This command is not in POSIX; it is bash's, so that `.bashrc` lines and bash
scripts that use it work. `shopt -s name` sets the option that does the
same in luish, as `setopt` does, and `shopt -u name` unsets it:

`autocd`
: `cd.auto`

`globstar`
: `glob.star`

`histverify`
: `history.verify`

Some of bash's options describe what luish always does, so they are always
on: `checkwinsize`, `cmdhist`, `expand_aliases`, `globasciiranges`,
`histappend`, `inherit_errexit`, `interactive_comments`, `lithist`,
`progcomp`, `promptvars`, `shift_verbose`, `sourcepath` and `xpg_echo`.
`login_shell` is on in a login shell and off otherwise. The others are
always off, as luish doesn't have them (such as `extglob`, `nullglob` and
`dotglob`). Setting an option that is always on, or unsetting one that is
always off, does nothing. The opposite is an error.

`-s`
: Set the options named (with names), or list those that are on.

`-u`
: Unset the options named (with names), or list those that are off.

`-o`
: The names are those of `set -o` (see `set` and `setopt`).

`-p`
: List the options as the commands that set them: `shopt -s name` or
  `shopt -u name`, and with `-o`, `set -o name` or `set +o name`. The
  output can be read back with `.` or `eval`.

`-q`
: Don't list anything: the exit status tells whether the options named
  are all on.

Without `-s` or `-u`, or without names, `shopt` lists the options named,
or all of them, as `name on` or `name off`.

The exit status is 0, except when an option named is off (when listing),
1 if a name doesn't exist, if an option can't be changed as asked, or if
both `-s` and `-u` are given, and 2 for an invalid option.
