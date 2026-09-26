# Extended globbing

luish can expand filename patterns as zsh does: `**/` matches any number of directories, and a *glob qualifier* in
parentheses after a pattern selects files by type, size, age and more, and can sort and change the result. Both are
off by default, so that scripts behave as in other POSIX shells. Turn them on with:

```sh
setopt globstar bareglobqual
```

For example:

```sh
ls **/*.md              # Markdown files in this directory and below
ls -d *(/)              # directories
ls *(.)                 # regular files
rm **/*.orig(.N)        # no error if there are none
vi *(.om[1])            # the most recently modified file
du -sh **/*(.Lm+100)    # files over 100 MiB
```

## Recursive globbing (`globstar`)

With `setopt globstar`, a path component that is exactly `**` and is followed by `/` matches zero or more
directories. `**/*.md` matches `x.md`, `a/y.md` and `a/b/z.md`; `a/**/` lists `a/` and every directory below it.

- Hidden directories (whose name starts with `.`) are not entered, unless the `D` qualifier is given. Symbolic links
  to directories are not followed. `***/` follows them, and stops at a link to a directory it is already in.
- `**` anywhere else (as the last component, or next to other characters, as in `a**`) is the same as `*`.
- A quoted `**` is not special.
- Without the option, `**` is the same as `*`, as in other POSIX shells.

As in zsh, the matches are sorted as full paths, so `a.md` comes before `a/b.md`.

## Glob qualifiers (`bareglobqual`)

With `setopt bareglobqual`, parentheses at the end of a word hold a list of qualifiers: `*(/)`, `**/*.c(.om)`. The
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
| `[`*n*`]`, `[`*n*`,`*m*`]` | only the *n*-th match, or the *n*-th to the *m*-th, after sorting. They count from 1, and negative numbers count from the end |
| `M` | add `/` after directories |
| `T` | add a character after each name for its type, as `ls -F`: `/` directory, `@` symbolic link, `*` executable, `\|` named pipe, `=` socket, `#` block device, `%` character device, and a space for other files |
| `:h`, `:t`, `:r`, `:e`, `:u`, `:l` | modifiers, at the end of the list: remove the last path component (head), keep only it (tail), remove the extension (root), keep only the extension, convert to upper or lower case |

A bad qualifier is an error, with status 1, as in zsh: `*(Z)` gives `unknown file attribute: Z`.

## Differences from zsh

- `**/` needs `setopt globstar`, and qualifiers need `setopt bareglobqual` (zsh has `**/` on by default, and
  qualifiers on outside its `sh` emulation).
- If nothing matches, the pattern is left as it is, as with `setopt nonomatch` in zsh (and in other POSIX shells).
- Subscripts count from 1, as in zsh's default mode (with `KSH_ARRAYS`, as in its `sh` emulation, they count from 0).
- Not supported yet: the `e`, `+`, `f`, `F`, `Y` and `P` qualifiers, `(#q...)`, modifiers other than those above,
  and the extended patterns of zsh's `EXTENDED_GLOB` (`^`, `~`, `#`). Unknown sort keys and modifiers are errors
  (zsh ignores some of them).
- Files that sort the same (with `o` or `O`) stay in name order; in zsh their order is unspecified.
