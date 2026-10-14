# Extended globbing

luish can expand filename patterns as zsh does: `**/` matches any number of directories, and a *glob qualifier* in
parentheses after a pattern selects files by type, size, age and more, and can sort and change the result. Options
also decide what a pattern that matches nothing gives, and whether patterns match hidden files (see
[below](#patterns-that-match-nothing-and-hidden-files)). All are off by default, so that scripts behave as in other
POSIX shells. Turn the first two on with:

```sh
setopt glob.star glob.bare_qualifiers
```

For example:

```sh
ls **/*.md              # Markdown files in this directory and below
ls -d *(/)              # directories
ls *(.)                 # regular files
rm **/*.orig(.N)        # no error if there are none
vi *(.om[0])            # the most recently modified file
du -sh **/*(.Lm+100)    # files over 100 MiB
```

## Recursive globbing (`glob.star`)

With `setopt glob.star`, a path component that is exactly `**` and is followed by `/` matches zero or more
directories. `**/*.md` matches `x.md`, `a/y.md` and `a/b/z.md`; `a/**/` lists `a/` and every directory below it.

- Hidden directories (whose name starts with `.`) are not entered, unless the `D` qualifier is given. Symbolic links
  to directories are not followed. `***/` follows them, and stops at a link to a directory it is already in.
- `**` anywhere else (as the last component, or next to other characters, as in `a**`) is the same as `*`.
- A quoted `**` is not special.
- Without the option, `**` is the same as `*`, as in other POSIX shells.

As in zsh, the matches are sorted as full paths, so `a.md` comes before `a/b.md`.

## Glob qualifiers (`glob.bare_qualifiers`)

With `setopt glob.bare_qualifiers`, parentheses at the end of a word hold a list of qualifiers: `*(/)`, `**/*.c(.om)`. The
parentheses must be the last thing in the word and contain no blanks, quotes or `$`. A word with a qualifier is a
pattern even if it has no `*`, `?` or `[`, so `file(N)` expands to `file` if it exists and to nothing otherwise.
Function definitions such as `f()` and `f( )` are not affected.

As in zsh's `sh` emulation, a qualifier can also come from an unquoted expansion: with `p='*(/)'`, `echo $p` lists
the directories. Quote the expansion (`"$p"`) to keep the text as it is.

Qualifiers that test files select the matches that pass every test. `,` separates alternatives: `*(/,@)` selects
directories and symbolic links.

| Qualifier | Selects |
|---|---|
| `/` | directories |
| `.` | regular files |
| `@` | symbolic links |
| `=` | sockets |
| `p` | named pipes (FIFOs) |
| `*` | executable regular files |
| `%`, `%b`, `%c` | device files, block devices, character devices |
| `r`, `w`, `x` | readable, writable, executable by the owner |
| `A`, `I`, `E` | readable, writable, executable by the group |
| `R`, `W`, `X` | readable, writable, executable by others |
| `s`, `S`, `t` | setuid, setgid, sticky |
| `U`, `G` | owned by the effective user, group |
| `u`*id*, `g`*id* | owned by the user or group *id*: a number, or a name between delimiters, as in `u:root:` |
| `d`*dev* | on device number *dev* |
| `l`[`-`\|`+`]*n* | with fewer than, more than or exactly *n* hard links |
| `L`[*unit*][`-`\|`+`]*n* | with a size less than, more than or equal to *n* units, rounded up. The unit is bytes, or `p` (512 bytes), `k`, `m`, `g` or `t` |
| `a`, `m`, `c` [*unit*][`-`\|`+`]*n* | accessed, modified or changed less than, more than or exactly *n* units ago. The unit is days, or `s`, `m`, `h`, `d`, `w` (weeks) or `M` (30 days) |

`^` negates the tests after it, and `-` makes the tests after it look at the target of a symbolic link rather than
the link: `*(-/)` also selects links to directories, and `*(-@)` selects broken links.

Other qualifiers change the result:

| Qualifier | Effect |
|---|---|
| `N` | if nothing matches, the word is removed (otherwise it is left as it is) |
| `D` | patterns match names starting with `.` (but never `.` and `..`), and `**/` enters hidden directories |
| `n` | names that contain numbers sort numerically (`f9` before `f10`) |
| `o`*key*, `O`*key* | sort in ascending or descending order: by name (`n`), size (`L`), number of links (`l`), time of access, modification or change (`a`, `m`, `c`, newest first), depth (`d`, files in subdirectories first) or not at all (`N`). Several keys can be given |
| `[`*n*`]`, `[`*n*`,`*m*`]` | only the *n*-th match, or the *n*-th to the *m*-th, after sorting. They count from 0, and negative numbers count from the end (`-1` is the last) |
| `M` | add `/` after directories |
| `T` | add a character after each name for its type, as `ls -F`: `/` directory, `@` symbolic link, `*` executable, `\|` named pipe, `=` socket, `#` block device, `%` character device, and a space for other files |
| `:h`, `:t`, `:r`, `:e`, `:a`, `:A`, `:u`, `:l` | modifiers, at the end of the list: remove the last path component (head; `:hN` keeps the first *N*), keep only it (tail; `:tN` the last *N*), remove the extension (root), keep only the extension, make the path absolute (also resolving symbolic links with `:A`), convert to upper or lower case. They are those of [parameter expansion](usage.md#parameter-expansion) |

A bad qualifier is an error, with status 1, as in zsh: `*(Z)` gives `*(Z): bad glob qualifier: unknown file attribute: Z`.
As in zsh with `globsubst`, the qualifier may come from an expansion, so `echo $PS1` fails if `PS1` ends in
`%(...)`; quote the expansion (`echo "$PS1"`) to keep it literal.

## Patterns that match nothing, and hidden files

In POSIX shells, a pattern that matches no file is left as it is: `echo *.txt` prints `*.txt`. Three options change
this, as zsh's options of the same names (and bash's) do:

`glob.null` (zsh's `NULL_GLOB`, bash's `nullglob`)
: The pattern is removed: `for f in *.txt` doesn't run its body when there are none, and `set -- *.txt` sets `$#`
  to 0. The `N` qualifier does this for one pattern.

`glob.fail` (zsh's `NOMATCH`, bash's `failglob`)
: The pattern is an error (`no matches found`, status 1), so the command doesn't run. As for other expansion
  errors, a script (or a subshell) exits; an interactive shell goes back to the prompt.

`glob.fail_all` (zsh's `CSH_NULL_GLOB`)
: The pattern is removed, as with `glob.null`, unless none of the patterns of the command (or of the words of `for`
  or of an array) matches: then it is an error, as with `glob.fail`. So `rm *.o *.a` removes whichever there are,
  and fails only if there are neither.

If more than one is on, `glob.null` wins, then `glob.fail_all`, as in zsh (in bash, `failglob` wins over
`nullglob`). They apply where patterns are expanded: the words of commands and `for`, and the elements of arrays. A
quoted pattern is not a pattern, and assignments, `case`, `[[ ... ]]` and redirections don't expand patterns. With
a qualifier, a pattern that matches nothing is as one without, unless the qualifier has `N`.

`glob.dots` (zsh's `GLOB_DOTS`, bash's `dotglob`) makes patterns match names starting with `.` (but never `.` and
`..`), as the `D` qualifier does: `*` lists hidden files too, and `**/` enters hidden directories. A pattern that
starts with `.` matches them anyway, and `.*` also matches `.` and `..`, as in dash (not in zsh, nor in bash 5.2).

## Differences from zsh

- `**/` needs `setopt glob.star`, and qualifiers need `setopt glob.bare_qualifiers` (zsh's `bareglobqual`; zsh has
  `**/` on by default, and qualifiers on outside its `sh` emulation).
- If nothing matches, the pattern is left as it is by default, as with `setopt nonomatch` in zsh (and in other POSIX
  shells); `setopt glob.fail` is zsh's default.
- With `glob.fail` or `glob.fail_all`, a pattern that matches nothing in `eval` exits a script, as other expansion
  errors do (as in dash); in zsh, `eval` returns 1.
- Subscripts count from 0, as in zsh's `sh` emulation (which sets `KSH_ARRAYS`); in its native mode they count from 1.
- Not supported yet: the `e`, `+`, `f`, `F`, `Y` and `P` qualifiers, `(#q...)`, modifiers other than those above,
  and the extended patterns of zsh's `EXTENDED_GLOB` (`^`, `~`, `#`). Unknown sort keys and modifiers are errors
  (zsh ignores some of them).
- Files that sort the same (with `o` or `O`) stay in name order; in zsh their order is unspecified.
