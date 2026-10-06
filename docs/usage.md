# Usage

If you have used bash or zsh, luish should feel familiar:

```bash
$ pwd
/home/luispedro
$ echo "hello world"
hello world
$ cd work/myproject
$ git status
On branch main
```

## Calling luish

luish accepts the usual `sh` invocations:

```sh
luish script.sh [args...]
luish -c 'command' [arg0 [args...]]
luish -s [args...]                # read commands from stdin
luish                             # interactive when stdin is a terminal
```

Options can be given as letters (`-e`, `-x`, ...) or with `-o name` / `+o name`.

On the command line, `-o` and `+o` take any option named as for `setopt`. Case
and `_` don't matter, and a `no` prefix inverts it (`-o err_exit`, `-o
no_glob`, `-o prompt_percent`; `+o glob` is the same as
`-o noglob`). luish also has these long options:

| Option | Effect |
|---|---|
| `--login` | The same as `-l`: a login shell, which reads `/etc/profile` and `~/.profile` (or `login.d`, see below) |
| `--interactive` | The same as `-i`: an interactive shell, even when standard input is not a terminal |
| `--stdin` | The same as `-s`: read commands from standard input; the operands are the positional parameters |
| `--no-rcs` | Don't read any startup files: `config.toml`, `rc.d`, `$ENV`, `luishrc`, and for a login shell `login.d` or `/etc/profile` and `~/.profile`. As zsh's `--no-rcs` |
| `--no-plugins` | Load no plugins: those that `config.toml` enables, and `plugin load` does nothing (as do `plugin add`, `plugin sync`, `plugin update` and `plugin check`), for example to check whether a problem comes from a plugin. The startup caches are neither used nor written |
| `--help` | Show a summary of the options and exit |
| `--version` | Show the version of luish and the git revision it was built from, and exit |

Options end at the first operand, or at `--` or `-`.

### Remote shells over SSH

`luish --ssh HOST` logs in to HOST with ssh and runs luish there, but edits command lines here: typing, moving in
the line, the history and the completion menu don't wait for the network. Commands run on HOST, and so do Tab
completion, the prompt and everything the startup files set up there.

luish needn't be installed on HOST: the first time, `luish --ssh` copies itself there, as
`~/.cache/luish/binaries/luish-VERSION-BUILD` (BUILD being the git commit it was built from) (in `$XDG_CACHE_HOME` if ssh's commands have it), and runs that copy
from then on. So each version (and build) of luish runs its own copy on HOST, whatever else is installed there.
Copies more than 30 days old are removed when a new one is made. If HOST is another system or architecture (as
`uname -sm` says), or the copy can't run there (a build linked with a newer C library than HOST's: the release
builds run on any Linux from 2014 on), the `luish` on the `PATH` that ssh gives commands is run instead (often
not the one a login shell has).

`--luish-path=PROGRAM` runs PROGRAM on HOST, and copies nothing (as rsync's `--rsync-path`). PROGRAM is run by the
remote user's shell, so it may start with `~/`; `--luish-path=luish` runs the one on the `PATH`.

```sh
luish --ssh myserver                  # copies luish to myserver, if needed
luish --ssh -p 2222 me@myserver       # ssh options go before the host
luish --ssh --luish-path='~/.local/bin/luish' myserver
luish --remote ssh -T myserver ~/.local/bin/luish --serve -l
```

`--remote` runs any command that starts `luish --serve` at the other end (and gives its standard input and output
to it); `luish --serve` takes the usual options, such as `-l` for a login shell. While a command runs, the keys go
to it as typed and its output comes back as it is printed, so full-screen programs such as vim work as they do
over ssh. The history is HOST's. Both ends must be the same version of luish (which `--ssh` makes sure of unless
given `--luish-path`).

The variables that describe the terminal (`TERM`, `COLORTERM`, `TERM_PROGRAM` and `TERM_PROGRAM_VERSION`) are
passed to HOST, replacing its own, and so is the locale (`LANG`, `LANGUAGE` and `LC_*`) where HOST has not set it.

## Shell language extensions

luish runs the POSIX shell language as dash does, and adds some of what bash
and zsh scripts use. These extensions only give a meaning to what is a syntax
error or a missing command in dash, so they are always on, and cost nothing in
a script that doesn't use them. Where the two shells differ, luish follows
zsh's `sh` emulation (`zsh --emulate sh`) and then bash; [](compatibility.md)
lists every difference.

```sh
function greet { echo "hello $1"; }        # the function keyword (also: function greet() { ...; })
if [[ $name == j* && -d $dir ]]; then ...  # conditions without word splitting or globbing
[[ $line =~ ^([a-z]+)=(.*)$ ]] && echo "${match[1]} ${match[2]}"   # a regular expression, with its groups
path+=(/opt/bin)                           # NAME+=value appends to a variable, or to an array
let 'n = n * 2 + 1'                        # arithmetic, with the status of its value
echo $((i++)) $((2**10)) $((a=1, a+1))     # ++ and -- (before or after), ** (power), and the comma operator
typeset -i count=0                         # variable attributes, also with declare
builtin cd /tmp                            # run the built-in even if a function has its name
set -o pipefail                            # a pipeline fails if any of its commands does
diff <(sort a) <(sort b)                   # process substitution: a file to read the output of a command
make |& less                               # pipe standard error too, as make 2>&1 | less
case $x in a) one;& b) two;| *) all;; esac # ;& runs the next body too, ;| (or ;;&) tries the next patterns
firefox &!                                 # in the background, and disowned (also &|)
read -r first rest <<< "$line"             # a here-string: the word and a newline on stdin
```

- `[[ ... ]]` has `test`'s operators, `&&`, `||`, `!` and parentheses, patterns on the right of `==` and `!=`, `=~`
  for regular expressions (which sets `MATCH`, `match` and `BASH_REMATCH`) and `-v NAME` for a set variable.
- Process substitution: `<(list)` is the path of a pipe (`/dev/fd/N`) that gives what the list writes, so a command
  that wants a file can read from a command. `>(list)` is one that feeds the list's standard input. The list runs
  in its own process, and the pipe is closed when the command that got it ends. luish then waits for a `>(list)`
  process, so its output comes before what follows (bash and zsh don't wait); a `<(list)` process isn't waited for.
  A redirection can take one too: `while read -r l; do ...; done < <(cmd)` (unlike `cmd | while ...`, the loop runs
  in the shell, so its variables stay), and `exec 3< <(cmd)` reads it later, from `<&3`. The word isn't split or
  globbed. It can be part of a word, as in bash and zsh: `prog --input=<(cmd)` passes `--input=/dev/fd/N`.
- [Arrays](#arrays), including associative ones, with `typeset`, `read -A` and zsh's parameter flags.
- [Parameter expansion](#parameter-expansion): `${x:offset:length}`, `${x/pattern/replacement}`, `${!name}`, zsh's
  modifiers (`${x:t}`, `${BASH_SOURCE:A:h}`) and more.
- [Special variables](#special-variables): `RANDOM`, `SECONDS`, `UID`, `pipestatus`, `path` and others.
- `source` is `.`, and looks for a name without a `/` in the current directory first, as zsh does.
- `shopt` and `zstyle` are not built-ins. For `shopt`, luish suggests the matching `setopt`.

## Special variables

Besides POSIX's (`$?`, `$$`, `$!`, `$-`, `$#`, `$0`, `$@`, `$*`, `LINENO`, `PPID`, `PWD`, `OLDPWD`, `OPTIND`,
`OPTARG`), luish has these from zsh, computed when they are read:

| Variable | Value |
|---|---|
| `RANDOM` | A random number from 0 to 32767. Assigning a number seeds the generator, which gives the same sequence as in zsh |
| `SECONDS` | The seconds since the shell started. Assigning a number makes it count from there |
| `EPOCHSECONDS`, `EPOCHREALTIME` | The time in seconds since 1970, and with microseconds (`1790530996.977661`) |
| `UID`, `EUID`, `GID`, `EGID` | The real and effective user and group ids |
| `HISTCMD` | The history event number of the command being run (0 without a history) |
| `pipestatus`, `PIPESTATUS` (bash's name) | An array of the statuses of the commands of the last pipeline: after `true \| false`, `${pipestatus[@]}` is `0 1`. As in zsh, every pipeline sets it, a single command or an `if` too, but not an assignment (so it survives `s=$?`) or `[[ ... ]]` |
| `path` | An array of the directories in `PATH`: `path=(~/bin "${path[@]}")` prepends one, and `path+=(/opt/bin)` appends one. An array assignment to it sets `PATH`, but `path=x` (valid in any POSIX shell) makes it an ordinary variable, as does `unset path`, and a `local path` is an ordinary variable of the function |
| `BASH_SOURCE` | An array of the files being run, innermost first, as in bash: the script, each file read with `.` (the path as given, or as found in `PATH`) and the startup files, and in a function the file the function was defined in. `$BASH_SOURCE` is the current one. It is unset in `-c` and when commands are read from standard input, and a function defined there has an empty file (bash has `main` or `environment`) |
| `FUNCNAME` | An array of the names of the functions being run, innermost first, as in bash, with `source` for a file read with `.` and `main` for the script. It is set only while a function runs |
| `BASH_LINENO` | An array of the lines each entry of `FUNCNAME` (and of `BASH_SOURCE`) was called from, as in bash: `${BASH_LINENO[0]}` is the line that called the current function, in the file `${BASH_SOURCE[1]}`. It is 0 for the script |
| `dirstack` | An array of the directory stack of `pushd` and `popd`, without the current directory (`dirs` shows it first): `${dirstack[0]}` is where `popd` goes. An array assignment (`dirstack=(~/src /tmp)`) replaces the stack; as for `path`, `dirstack=x`, `unset dirstack` and `local dirstack` make it an ordinary variable |
| `LUISH_VERSION` | luish's version (`0.5.0`), as zsh's `ZSH_VERSION` and bash's `BASH_VERSION` |
| `LUISH_PATCHLEVEL` | The git commit luish was built from, with `-dirty` if the sources had changes (`unknown` outside a git checkout), as zsh's `ZSH_PATCHLEVEL` |
| `MACHTYPE`, `HOSTTYPE` | The processor (`x86_64` or `aarch64`), as zsh's `MACHTYPE` and bash's `HOSTTYPE` |
| `OSTYPE` | The operating system: `linux-gnu`, or `linux-musl` for the musl build |
| `SHLVL` | How deeply shells are nested: incremented at startup, and set to 1 by an interactive shell where it wasn't set |

Their values in the environment are ignored, and they are not exported, so `[ -n "$LUISH_VERSION" ]` tells a script
whether luish runs it. Unset, they read as unset until they are assigned again. Assigning to one other than `RANDOM`
and `SECONDS` makes it an ordinary variable, so that scripts that use these names still work. Made ordinary by `local` in a function or by an assignment before a command, it is special again afterwards.

## Parameter expansion

Besides POSIX's forms (`${x:-word}`, `${x#pattern}`, `${#x}` and so on), luish has these, as zsh and bash:

| Form | Value |
|---|---|
| `${x:offset}`, `${x:offset:length}` | The part of `x` from `offset` (counting from 0), of at most `length` bytes. Both are arithmetic expressions. A negative offset counts from the end (write `${x: -1}` or `${x:(-1)}`, as `${x:-1}` is the default value), and a negative length leaves out that many bytes at the end |
| `${x/pattern/replacement}` | `x` with the first (longest) match of the pattern replaced; without `/replacement`, removed |
| `${x//pattern/replacement}` | Every match replaced |
| `${x/#pattern/replacement}`, `${x/%pattern/replacement}` | A match at the start, or at the end, replaced |
| `${x:h}`, `${x:t}`, `${x:r}`, `${x:e}` | zsh's modifiers of file names: the directory (`/usr/lib` for `/usr/lib/a.tar.gz`; `.` if there is no `/`), the last component (`a.tar.gz`), without the extension (`/usr/lib/a.tar`), the extension (`gz`). Trailing slashes are ignored by `h` and `t`. `:hN` keeps the first `N` components (`${x:h2}` is `/usr`) and `:tN` the last `N` |
| `${x:a}`, `${x:A}` | An absolute path, with `.` and `..` removed (from the text, relative to the current directory); with `A`, also with symbolic links resolved, as far as the path exists |
| `${x:u}`, `${x:l}` | Upper case, lower case |

For `$@` and `$*`, `${@:offset:length}` selects positional parameters (offset 0 is `$0`), and `${@/pattern/rep}`
replaces in each of them (`"${*/pattern/rep}"` replaces in the joined string, as in zsh).

Modifiers can follow each other, applied from left to right: `${x:t:r}` is the name of the file without its
extension. On `$@`, `${a[@]}` and the like they apply to each element (to the joined string in `"${*:t}"` and
`"${a[*]:t}"`). The same modifiers work in [glob qualifiers](globbing.md) (`*(:t)`) and, except `u` and `l`, in
[history expansion](#history-expansion).

`${BASH_SOURCE:A:h}` is the directory of the file being run, with symbolic links resolved: where a script finds the
files that come with it, without `$(dirname "$(readlink -f "$0")")`.

```sh
here=${BASH_SOURCE:A:h}
. "$here/lib.sh"
```

Since `BASH_SOURCE` is the path as it was given to the shell or to `.`, take it before changing directory if it may
be relative.

## Arrays

As in zsh and bash, a variable can hold an array: a list of strings, indexed from 0 (as in zsh's `sh` emulation
and bash; native zsh counts from 1).

```sh
files=(*.txt "my notes" ~/todo)      # the elements are expanded as command words
files+=(extra)                       # append elements
files[1]=other                       # assign an element (from the end if negative)
echo "${files[0]}" "${files[-1]}"    # an element; the index is an arithmetic expression
for f in "${files[@]}"; do ...; done # each element as a separate word, as "$@"
echo "${#files[@]}"                  # the number of elements
local list=(a b c)                   # also with local, export, readonly and typeset
typeset -a empty                     # an empty array (typeset -p prints variables)
read -A words                        # read the fields of a line (bash: read -a words)
```

| Form | Value |
|---|---|
| `${a[i]}` | Element `i` (counting from the end if `i` is negative). `$a` is `${a[0]}` |
| `"${a[@]}"`, `"${a[*]}"` | The elements as separate words, or joined with the first character of `IFS`, as `"$@"` and `"$*"` |
| `${#a[@]}`, `${#a[i]}` | The number of elements, the length of an element |
| `"${!a[@]}"`, `"${!a[*]}"` | The indices (`0 1 2 ...`), or the keys of an associative array, as in bash |
| `${!x}` | Indirection, as in bash: the parameter named by the value of `x`, which may be `name[index]` or a positional or special parameter. Operators apply to that parameter (`${!x:-default}`), and `${!a[i]}` goes through an element |
| `"${!prefix@}"`, `"${!prefix*}"` | The names of the set variables that start with `prefix`, sorted, as in bash |
| `${a[@]:offset:length}` | The elements from `offset` on (at most `length` of them) |
| `"${a[i..j]}"` | A slice, as in Python: the elements from `i` up to (not including) `j`, as separate words, as `"${a[@]}"`. Either end can be left out (`${a[2..]}`, `${a[..-1]}`), a negative one counts from the end, and they are clamped to the array, so `${a[-2..]}` is the last two elements (or fewer). The ends are arithmetic expressions, and an unquoted `..` separates them |
| `${a[@]#pattern}`, `${a[@]/pattern/rep}`, ... | The operator applied to each element (also to a slice, `${a[1..3]#pattern}`) |

Arrays have no holes: assigning past the end fills the gap with empty elements, and `unset 'a[i]'` makes an element
empty, as in zsh. A string is an array of one element, so `${s[0]}` is `$s`. Arrays aren't exported to commands.
Arithmetic expressions can use elements, as in `$((a[i] + 1))` and `a[i] += 2`. In a list, `[i]=value` gives the
index of an element, and those that follow come after it: `a=([2]=x y)` is `('' '' x y)`.

An associative array, made with `typeset -A` (or `local -A`), maps keys to values. A key is any string: the subscript
is expanded as a word is in an assignment, not evaluated as arithmetic, and `\]` in it is a `]`.

```sh
typeset -A size                      # an empty associative array
size[small]=1                        # assign a value
size=([small]=1 [big]=10)            # replace them all (also as pairs: size=(small 1 big 10))
size+=([huge]=100)                   # add keys
echo "${size[big]}" "${#size[@]}"    # a value, and the number of keys
for v in "${size[@]}"; do ...; done  # the values
for k in "${!size[@]}"; do ...; done # the keys (bash; also "${(k)size[@]}", as in zsh)
echo $((size[small] + size[big]))    # in arithmetic, the text of the subscript is the key
unset 'size[huge]'                   # remove a key
```

The operators apply to the values as they do to the elements of an array. The values come in no particular order (in
luish, the order in which the keys were added, until one is removed; zsh and bash use another). As in bash, `$h` is
`${h[0]}`, the value at key `0`, and `h=value` assigns to it. `read -A` reads pairs of keys and values into one.

zsh's parameter flags, in parentheses after `${`, transform the value of a parameter (or the elements of an array,
with `[@]`):

```sh
echo "${(j:,:)files[@]}"             # the elements joined with commas
parts=(${(s:/:)PWD})                 # split at each / (the words aren't split again)
for k in "${(ko)size[@]}"; do ...; done   # the keys, sorted
echo ${(u)list[@]} ${(Oa)list[@]}    # without repeated elements; in reverse order
```

| Flag | Effect |
|---|---|
| `j:sep:`, `F` | Join the words with `sep`, or with newlines |
| `s:sep:`, `f` | Split into words at each `sep` (at each byte if it is empty), or at newlines |
| `L`, `U`, `C` | Lower case, upper case, or a capital at the start of each word (of letters and digits) |
| `u` | Only the first of repeated words |
| `o`, `O` | Sort, in ascending or descending order (byte order), also with `i` (ignoring case) and `n` (numbers by their value: `x2` before `x10`); `i` or `n` alone sort too |
| `a` | The array's order (`Oa` reverses it) |
| `k`, `v` | For an associative array, its keys, or its values (the default); both give each key followed by its value |
| `@` | Separate words even in double quotes: `"${(@)a[*]}"` is `"${a[@]}"` |

The delimiters around `sep` can be any character, or a pair of brackets: `(j(, ))`. The operator, such as `:-` or
`/`, applies first, then `j`, `s`, the case, `u` and the order, whatever order the flags are written in. As in zsh, in
double quotes and where the result is a single word (`x=${(o)a[@]}`), the elements of `$*` and `${a[*]}` (and in
the latter case of `${a[@]}`) are joined first, unless `@` or `j` is given, and there `s` doesn't split.

`typeset -i` gives variables the integer attribute, as in zsh and bash: what is assigned to them is evaluated as an
arithmetic expression (for an array, each element), so `typeset -i n=2*3` sets `n` to `6`, and `n+=1` adds.
Likewise, `typeset -l` and `typeset -u` convert what is assigned to lower or upper case, and zsh's `typeset -U` keeps
only the first of equal elements in an array: `typeset -U path` keeps `PATH` free of repeated directories.

## Error messages

An error message names the file of the code that failed and its line, then shows the text of that line and how the
shell got there: a line for each function call and each file read with `.`, innermost first, with where it was
called and the text of that line.

```text
./lib.sh: 2: nosuchcmd: not found
    nosuchcmd
  in function load_config, called at main.sh:3
      load_config
  in function setup, called at main.sh:5
      setup
```

dash names the script (`$0`) instead, even for an error in a file it read with `.`, and shows neither the line nor
the stack. The lines are read again from their files when the error happens (so a file changed since shows its new
text), or from the `-c` command; they aren't shown for code typed at the prompt or run by `eval` or a trap, and a
long line is cut. A line of the stack that repeats (in recursion) is shown once with a count, and a stack of
more than 20 lines loses its middle. In `-c`, calls are at lines of the command (`called at line 2`), and at the
prompt of an interactive shell the stack has no lines. A function that the startup cache or a saved state restored
keeps the lines of its file.

In an interactive shell whose standard error is a terminal, the names of files are links to them (OSC 8), which
terminals that know them (kitty, WezTerm, Ghostty, foot, iTerm2, GNOME Terminal and other VTE ones, Windows Terminal)
open when clicked (with Ctrl or a modifier in some); `setopt terminal.no_integration` turns them off.

`caller` prints a frame of the stack from a script, as in bash (see `help caller`), and `BASH_SOURCE`, `FUNCNAME`
and `BASH_LINENO` hold all of it ([Special variables](#special-variables)).

## Tracing variables

With variable tracing on, `where` tells where variables were set (see `help where`):

```text
$ luish -o vars.trace
$ where PATH EDITOR
PATH was set to "/home/me/bin:/usr/bin:/bin" in ~/.config/luish/rc.d/10-path.lsh:3
EDITOR was set to "vi" in ~/.my-script.sh:123, in function setup
```

A value too long for the terminal goes on a line of its own, below, and where it was set on the next. On a terminal,
names, values, files and functions are in colour, in the styles `var`, `string`, `path`, `command.function` and
`comment` (the numbers of changes) of the line editor's highlighting.

`setopt vars.trace` records where each variable was last set: the file and line of the code that set it and the
function running, or the prompt, the `-c` command or standard input. `setopt vars.trace_history` also keeps the last
100 changes of each variable, with their values, which `where -a` shows. With `vars.trace`, the changes of `PATH`,
`MANPATH`, `PS1`, `RPROMPT` and `RPS1` are kept as in history mode, since startup files and plugins often build them
bit by bit:

```text
$ where -a PATH
[1] PATH was inherited from the environment as
    "/usr/bin:/bin"
[2] PATH was set to
    "/home/me/bin:/usr/bin:/bin"
    in ~/.config/luish/rc.d/10-path.lsh:3
[3 - current state] PATH was set to
    "/home/me/bin:/usr/bin:/bin:/opt/x/bin"
    in ~/.local/share/luish/plugins/x/init.lsh:7, in function add
```

Tracing costs nothing while it is off. While it is on, assignments are slower (a script of function calls and
assignments takes about a fifth longer with `vars.trace`, a third with `vars.trace_history`), and the [startup caches](#cached-startup-files) aren't used, so that
the startup files run and what they set is recorded at their lines. Turn it on with `-o` on the command line to trace
the startup files; with `setopt`, only what is set from then on is recorded (a variable set before shows as set
before tracing began). Values that a plugin's `prompt-vars` sets for the prompt are put back after it, and aren't
recorded. Tracing in a subshell stays in that subshell.

## Getting help

In an interactive shell, `help` lists the built-in commands, and `help NAME` shows the help for any of them (the
same text as in [](builtins.md)).

## Prompts

`PS1` is the prompt, `PS2` the prompt for the continuation lines of a command, and `PS4` the prefix of the lines
that `set -x` prints. As POSIX requires, they go through parameter expansion, so `PS1='$PWD\$ '` shows the current
directory.

With the `prompt.percent` option (`setopt prompt.percent`), they then also expand `%` sequences, as in zsh:

```sh
setopt prompt.percent
PS1='%[fg:blue]%[dir]%[fg_off] %([status]..%[fg:red][%[status]]%[fg_off] )%[prompt_char] '
```

This shows the current directory (with `~` for `$HOME`) in blue, then the exit status of the last command in red if
it failed, then `#` for root and `%` for other users. Parameter expansion comes first, so a `%` in the value of a
variable is expanded too; write `%%` for a literal `%`. The sequences are those of zsh:

| Sequence | Expands to | Long name |
|---|---|---|
| `%%`, `%)` | `%`, `)` | `%[percent]` |
| `%~`, `%d` or `%/` | The current directory, with or without `~` for `$HOME`. With a number, `%N~` gives only its last `N` components, and `%-N~` its first `N` | `%[dir]`, `%[pwd]` |
| `%c` or `%.`, `%C` | The last component of the current directory, with or without `~` (`%Nc` for more) | `%[dir_tail]`, `%[pwd_tail]` |
| `%n`, `%m`, `%M` | The user name, the host name up to the first `.` (`%Nm`: `N` components), the full host name | `%[user]`, `%[host]`, `%[hostname]` |
| `%#` | `#` for root, `%` otherwise | `%[prompt_char]` |
| `%?` | The exit status of the last command | `%[status]` |
| `%h` or `%!` | The number of the next history event | `%[history]` |
| `%j` | The number of jobs | `%[jobs]` |
| `%L`, `%i` | `$SHLVL`, the line number (for `PS4`) | `%[shlvl]`, `%[lineno]` |
| `%l`, `%y` | The terminal, without `/dev/` (and, for `%l`, without `tty`) | `%[tty_short]`, `%[tty]` |
| `%D`, `%T`, `%*`, `%t` or `%@`, `%w`, `%W` | The date as `yy-mm-dd`, the time as `HH:MM` or `HH:MM:SS`, or in 12-hour format, the weekday and day, the date as `mm/dd/yy` | `%[date]`, `%[time]`, `%[time_seconds]`, `%[time_12h]`, `%[date_weekday]`, `%[date_us]` |
| `%D{format}` | The time in a `strftime` format (and zsh's `%f`, `%K` and `%L`, the day and hours without padding) | `%[date:format]` |
| `%B` `%b`, `%U` `%u`, `%S` `%s` | Start and stop bold, underline and standout (reverse video) | `%[bold]` `%[bold_off]`, `%[underline]` `%[underline_off]`, `%[standout]` `%[standout_off]` |
| `%F{colour}` `%f`, `%K{colour}` `%k` | Start and stop a foreground and a background colour: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, the same with `bright-`, `default`, a number from 0 to 255, or `#rrggbb`. `%NF` is `%F{N}` | `%[fg:colour]` `%[fg_off]`, `%[bg:colour]` `%[bg_off]` |
| `%E` | Clear to the end of the line | `%[clear_eol]` |
| | Start the style `name` (see [Styles](#styles)), and go back to the attributes and colours before it | `%[style:name]` `%[style_off]` |
| `%{...%}` | Text written as it is, taking no room on the screen: for other escape sequences, such as a terminal title | |
| `%NG` | Within `%{...%}`: the escape sequence takes `N` columns (at most 65536) | |
| `%(x.yes.no)` | `yes` if the condition `x` holds, otherwise `no` (any character can replace the `.`s). The conditions take a number `N`, as in `%(N?.yes.no)` or `%N(?.yes.no)`: `?` the exit status is `N` (0 by default), `#` the user id is `N` (0: root), `!` the shell runs as root, `g` the group id is `N`, `j` there are at least `N` jobs, `L` `$SHLVL` is at least `N`, `/` or `C` the current directory has at least `N` components, `~`, `.` or `c` the same, with `~` for `$HOME` counting as one; `T`, `t`, `d`, `D` and `w`: the hour, minute, day of the month, month (from 0 for January) or day of the week (from 0 for Sunday) is `N` | `%([name].yes.no)` |
| `%N<text<`, `%N>text>` | Shorten what follows (up to the end of the enclosing `%(...)`, or to the next `%<<`) to `N` characters, replacing what is cut on the left or the right by `text` | |

Other sequences expand to nothing. zsh's `%_`, `%e`, `%I`, `%N`, `%x`, `%v`, and conditions and truncation widths
relative to the terminal's width, aren't supported.

The long names are luish's own (zsh has none): `%[name]` is the same as the short sequence, easier to read in a long
prompt. The argument of a sequence goes after a `:`, as a number or as the text in braces: `%[dir:2]` is `%2~`,
`%[fg:red]` is `%F{red}` and `%[date:%H:%M]` is `%D{%H:%M}` (a `\` quotes a `]`). The number can also come first, as
in `%2[dir]`. Case, `_` and `-` don't matter, so `%[HostName]` is `%[hostname]`.

Named styles are the same as the line editor's, so a colour scheme can colour the prompt as well as the command
line, and a prompt can use the colours of the line (`%[style:command]`). Names of your own, such as `prompt.dir`,
are free:

```sh
style prompt.dir bold blue
style prompt.error red
PS1='%[style:prompt.dir]%[dir]%[style_off] %([status]..%[style:prompt.error][%[status]]%[style_off] )%[prompt_char] '
```

A style is added to the attributes and colours already in effect, as a style is to its parent (so `%[bold]` then a
style that only sets a colour gives bold text in that colour, unless the style is `plain`), and `%[style_off]` goes
back to what was in effect before the matching `%[style:...]`, or to the terminal's defaults; escape sequences in
`%{...%}` aren't followed. With a non-empty `$NO_COLOR`, named styles do nothing (but `%[fg:...]` and the like
still work, as you asked for them by colour).

The conditions of `%(...)` have long names too, in brackets, with their number after a `:`: `%([status:1].yes.no)`
is `%(1?.yes.no)`. They are `status` (`?`), `root` (`!`), `uid` (`#`), `gid` (`g`), `jobs` (`j`), `shlvl` (`L`),
`pwd` (`/`), `dir` (`~`), `hour` (`T`), `minute` (`t`), `day` (`d`), `month` (`D`) and `weekday` (`w`).

Unlike an unknown short sequence, an unknown long name is an error, written to stderr each time the prompt is
expanded (it then expands to nothing). It suggests the closest name, or else lists them:

```text
luish: unknown prompt sequence %[hostnam]; did you mean %[hostname]?
```

zsh's deprecated form of truncation, `%[N<text]` (a `[` followed by a number or by `<` or `>`), is `%N<text<`.

### The right prompt

As in zsh, `RPROMPT` (or `RPS1`) is a prompt shown at the right edge of the terminal, on the last line of `PS1`, and
`RPROMPT2` (or `RPS2`) one that goes with `PS2`. They are expanded as `PS1` is, with `%` sequences if
`prompt.percent` is on:

```sh
setopt prompt.percent
RPROMPT='%[fg:yellow]%[time]%[fg_off]'
```

The right prompt makes way for the command: it is shown only while the line, and its autosuggestion, leave a column
free before it, and comes back if the line gets shorter. It leaves `$ZLE_RPROMPT_INDENT` columns (1 by default) free
at the right edge, as in zsh; some terminals need it, to avoid scrolling when something is written in the last
column. A right prompt that doesn't fit on one line isn't shown.

By default the right prompt stays on the screen next to the commands that were run, as in zsh. With `setopt
prompt.transient_rprompt` (zsh's `TRANSIENT_RPROMPT`) it is removed when a command is accepted.

For what `PS1` can't compute by itself, such as the git branch, a plugin can provide variables for it to use, which
are set only while the prompt and the right prompt are built (see
[Customizing the prompt](plugins.md#customizing-the-prompt)).

## Line editing

An interactive shell edits command lines with emacs keys, as zsh does, or with vi keys after `set -o vi` (or
`bindkey -v`). The emacs keys are zsh's, and `bindkey` shows and changes them (see `help bindkey`). Some of the most
useful:

| Key | Action |
|---|---|
| Up, Down | The previous or next command that starts with the text before the cursor (all commands if the line is empty); Down past the newest brings back what was typed |
| Ctrl-P, Ctrl-N | The previous or next command |
| Ctrl-R | Search the history as you type |
| Alt-. | Insert the last word of the previous command; again, that of the one before |
| Ctrl-O | Run the line, and start the next one with the command after it in the history, to run a series of commands again |
| Ctrl-W, Alt-Backspace | Delete the word before the cursor |
| Alt-B, Alt-F, Alt-D | Move back a word, forward to the next word, or delete to the end of the word |
| Ctrl-A, Ctrl-E | Go to the start or end of the line |
| Ctrl-K, Ctrl-U | Delete to the end of the line, or the whole line |
| Ctrl-Y, Alt-Y | Put back what was deleted, or instead what was deleted before it |
| Ctrl-_ | Undo |

As in zsh, words are made of letters, digits and the characters in `WORDCHARS`, by default
`*?_-.[]~=/&;!#$%^(){}<>`. Many zsh users leave out `/`, so that Ctrl-W deletes one directory of a path:

```sh
WORDCHARS='*?_-.[]~=&;!#$%^(){}<>'
```

With `setopt editor.autosuggest`, the line editor suggests the rest of the newest command in the history that starts
with what has been typed, in grey after the cursor, as the zsh-autosuggestions plugin does. Right, End, Ctrl-F or
Ctrl-E accept the suggestion, and Alt-F accepts its next word. Its colour is the `suggestion` style (see [Syntax
highlighting](#syntax-highlighting); by default grey, `bright-black`).

To bind Up and Down as zsh does by default:

```sh
bindkey Up up-line-or-history
bindkey Down down-line-or-history
```

Keys can be named (as `Up`, `Ctrl-Right`, `Alt-.` or `'Ctrl-X Ctrl-E'`) or written as zsh writes them (`'^[[A'`),
and bindings can also go in `config.toml` (see below).

## Syntax highlighting

The line editor colours the command line as it is typed. `setopt editor.no_highlight`, or a non-empty `$NO_COLOR`,
turns it off.

A syntax error is marked from where it is to the end of the line, in the style `error` (added to the colours the
text has anyway; red and underlined by default), as the shell would report it if the line were entered then: a
line that is only incomplete, which would get the `PS2` prompt, has no error. A word with an error isn't marked while
the cursor is at its end, as it may still be being typed (`do` on the way to `docker`), nor is an operator missing
what follows it while the cursor is after it (`ls >` before the file name). Lines over 64 KiB (such as a long
paste) aren't checked.

With `setopt highlight.paths`, an argument or a redirection's target that names a file that exists is marked, in
the style `path` (underlined by default), and so is the word under the cursor if it begins the name of one, in the
style `path.prefix` (by default as `path`). Only words with no expansions count, except a leading `~` for `$HOME`
(`~user/x` doesn't); quoted words do, but not options (`-x`). This is off by default since it means looking at the
file system on each key, which can be slow on a network file system: what is looked up is remembered until the next
prompt, and each redraw looks up at most 16 new names, leaving other words to the next key.

### Styles

Each part of the line has a style, by name. The names are dotted, and a style that isn't set takes what it doesn't
say from its parent: `command.unknown` from `command`, for instance. These are the names luish highlights with so
far:

| Name | For |
|---|---|
| `keyword` | reserved words (`if`, `for`, `{`, `[[`) |
| `command.builtin` | built-ins, also those of plugins |
| `command.function` | functions (also those defined earlier on the line), and the name in a function definition |
| `command.alias` | aliases, also suffix aliases (and those defined earlier in the text, from the next complete command on) |
| `command.external` | commands found in `PATH`, or by a path with a `/` |
| `command.precommand` | commands that run the next word as a command (`sudo`, `env`, `exec`, `command`, `nohup`) |
| `command.directory` | directories, with `setopt cd.auto` |
| `command.history` | history references, with `setopt history.expand` (`!!`, `^a^b`) |
| `command.unknown` | command names that aren't found (not marked while the cursor is on them) |
| `arg` | arguments |
| `arg.option` | arguments starting with `-` |
| `string.single` | `'...'` |
| `string.double` | `"..."` |
| `string.escape` | backslash escapes (`\ `, and `\$` in double quotes) |
| `string.heredoc` | the text of here-documents |
| `var` | `$NAME` and `${...}` |
| `var.special` | special and positional parameters (`$?`, `$1`, `${10}`) |
| `var.exported` | exported variables |
| `var.array` | arrays, also associative ones (`${a[1]}`) |
| `var.readonly` | read-only variables (an exported or array one too) |
| `var.unset` | `$NAME` or `${NAME}` when `NAME` is not set (and not assigned earlier on the line, or in a `for` loop there) |
| `subst.command` | the delimiters of `$(...)` and backquotes |
| `subst.process` | the delimiters of `<(...)` and `>(...)` |
| `subst.arith` | `$((...))` |
| `expand.tilde` | `~` and `~user` |
| `expand.brace` | the braces and separators of a brace expansion, with `setopt expand.braces` (`{a,b}`, `{1..3}`) |
| `expand.glob` | pattern characters (`*`, `?`, `[...]`) in arguments and `case` patterns, unless `set -f` |
| `op.control` | `;`, `&`, `&&` and `\|\|` |
| `op.pipe` | `\|` |
| `op` | other operators (`(`, `)`, `;;`) |
| `redir` | redirection operators, and here-document delimiters |
| `redir.fd` | the file descriptors in redirections (`2>&1`) |
| `comment` | comments |
| `assign` | the `NAME=` of an assignment |
| `menu.selected` | the selection in the completion menu |
| `menu.description` | descriptions in the completion menu |
| `suggestion` | autosuggestions |
| `plugin.name`, `plugin.ok`, `plugin.update`, `plugin.warn`, `plugin.error`, `plugin.dim` | the output of `plugin`, on a terminal |

So `command` sets all the kinds of command at once, `string` all the strings, and so on. `error` is added to the
style of the text with a syntax error, and `path` and `path.prefix` to that of a word that names a file (see
above). More names exist already, for distinctions the highlighter doesn't make yet (`style` lists them). Other names, such as `git.branch` or `prompt.dir`, are free for plugins and prompts (`%[style:prompt.dir]`, see
[Prompts](#prompts)) to use, but not those that start like a name of luish's (`command.nosuch` is an error).

A style's value is words separated by spaces:

- a colour for the text: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, the same with
  `bright-` (`bright-black` is grey), `default`, a number from 0 to 255, or `#rrggbb` (as in `%F{...}` in prompts);
- `bg:` and a colour for the background;
- attributes: `bold`, `dim`, `italic`, `underline`, `blink`, `reverse`, `strike`, and the same with `no-` to turn
  one off that the parent has;
- other kinds of underline, as in vim: `undercurl`, `underdouble`, `underdotted` and `underdashed` (each underlines,
  so `no-underline` turns it off), and `ul:` and a colour for the underline's colour, as in `undercurl ul:red`. kitty,
  WezTerm, Ghostty, foot, iTerm2, Alacritty and VTE terminals show them; others show a straight underline in the
  text's colour, or nothing;
- `plain`: the terminal's defaults, taking nothing from the parent;
- `sgr:` and the parameters of a terminal escape sequence, such as `sgr:1;38;5;208`.

The `style` built-in shows and changes them (see `help style`):

```sh
style command.unknown            # its value, and where it comes from
style command.unknown bold red   # set it
style -r command.unknown         # back to the colour scheme's
```

### Colour schemes

A colour scheme is a named set of styles. luish has two, `default-dark` and `default-light` (the same but for the
strings, as yellow is hard to read on a light background), and more can be defined in `config.toml`, by a plugin, or
with `style -s`. A scheme can inherit what it doesn't set from another:

```toml
[colorscheme.blue]
keyword = "bold blue"
command = "blue"
"command.unknown" = "bold red"
string = "yellow"
var = "cyan"
"var.unset" = "dim cyan"

[colorscheme.green]
inherits = "blue"
keyword = "bold green"
subst = "magenta"

[style]
colorscheme = "green"
"command.function" = "bold"     # over the scheme
```

`colorscheme` in the `style` table chooses the scheme, and the other keys there set styles over it (as `style NAME
VALUE` does). A scheme starts empty: it has only what it sets and what it inherits. Each name's value comes from the
first of: the `style` table (or the `style` built-in), the scheme, the schemes it inherits from, then the defaults
of plugins; then what it leaves out comes from its parent the same way. So a scheme's `command.unknown` is kept even
if `[style]` sets `command`.

The scheme can depend on whether the terminal's background is dark or light:

```toml
[style]
colorscheme = { dark = "green", light = "blue", default = "green" }
```

`default` is for when the background isn't known; without it, `dark` is used. The default choice is `{ dark =
"default-dark", light = "default-light" }`. luish knows the background from `$LUISH_BACKGROUND`, if it is `dark` or
`light`, or else from `$COLORFGBG`, which some terminals (rxvt, Konsole) set, or else by asking the terminal for its
background colour before the first prompt, which most terminals answer (xterm, GNOME Terminal and other VTE ones,
Konsole, kitty, Alacritty, WezTerm, foot, iTerm2, Ghostty, Windows Terminal). It puts what it found in
`$LUISH_BACKGROUND`, so set that variable (in `luishrc`, or in the environment) to say it instead, for a terminal
that answers wrongly. Keys typed while the shell waits for the answer start the command line. `style --detect` asks
again, after the terminal's colours have changed. `style -c` lists the schemes and shows which is in use, and why;
`style -c NAME` (or `style -c DARK LIGHT [DEFAULT]`) chooses.

[](colour-schemes.md) shows how to make a scheme, step by step.

### The terminal's colours

A scheme can also set the terminal's own colours, which the programs you run use too: its background, its text
colour, its cursor's colour and the colours of its palette (the 16 that `ls --color` and `git diff` use). They go in
the scheme's `terminal` table:

```toml
[colorscheme.ocean.terminal]
background = "#1c2331"
foreground = "#d8dee9"
cursor = "#d8dee9"
palette = ["#1c2331", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d8dee9"]
```

Each colour is `#rrggbb` (or `#rgb`). `palette` gives colours 0 to 15 in order, or fewer. A scheme inherits these
from the one it `inherits` from, each on its own, as it does styles; with `style -s`, they are `terminal.background`
and so on (`style -s ocean terminal.cursor '#ffcc00'`).

While a scheme that gives them is in use, the shell sets them before the prompt, with the escape sequences that
most terminals take (OSC 10, 11, 12 and 4: xterm, GNOME Terminal and other VTE ones, kitty, foot, Alacritty,
WezTerm, iTerm2), if its standard input and error are a terminal. Before it first sets one, it asks the terminal
what it was, and it puts that back when the scheme in use no longer sets it, when the shell exits, and before `exec`
runs another program; a colour the terminal didn't tell is reset to the terminal's default instead. So a shell
started from another one puts back that one's colours. When the shell finds out the background (for a dark/light
pair, or with `style --detect`), it asks for the terminal's own, so it puts back its colours first.

The colours stay if the shell doesn't exit cleanly, as when it is killed or an ssh connection drops; `printf
'\e]104\a\e]110\a\e]111\a\e]112\a'` resets them. tmux and screen may not pass the sequences on to the terminal.

To keep the terminal's own colours with any scheme:

```toml
[style]
terminal-colors = false     # or terminal-colours
```

or `style --terminal-colors off` in a running shell. A non-empty `$NO_COLOR` turns them off too.

## Terminal integration

In the line editor, the shell tells the terminal where each prompt, command line and command output is, and the
exit status of each command (the semantic prompt marks, OSC 133), and the current directory whenever it changes
(OSC 7). Terminals that know them use them: kitty, WezTerm, Ghostty, foot, iTerm2, Windows Terminal, VS Code's
terminal and tmux (3.4 or later) can jump from prompt to prompt, select or copy a command's output, mark the
commands that failed, or open a new window or tab in the same directory (GNOME Terminal and other VTE ones need only
OSC 7 for that). Other terminals ignore them. In vi mode (`set -o vi`), the cursor is a bar while inserting, a block
in command mode and an underline while replacing, and the terminal's own cursor is put back while commands run.
Error messages make the names of files links to them (see [Error messages](#error-messages)).
`setopt terminal.no_integration` turns all of this off.

With the marks, kitty and Ghostty can send a desktop notification when a long command ends while their window isn't
focused: `notify_on_cmd_finish unfocused` in `kitty.conf`, or `notify-on-command-finish = unfocused` (with
`notify-on-command-finish-action = notify`) in Ghostty's configuration (version 1.3 or later). For other terminals that
show notifications, the plugin [`std/notify`](plugins.md) asks for one itself.

`clipcopy` puts its input (or a file) on the clipboard through the terminal (OSC 52), which also works over ssh:
`git rev-parse HEAD | clipcopy`. See `help clipcopy`.

## History

An interactive shell keeps the last `HISTSIZE` commands (1000 by default) in
its history, where the line editor (Up and Down, Ctrl-R, Alt-.) and `fc` find
them.

A command the same as the one before it is not added again. By default, history
is saved to `$XDG_STATE_HOME/luish/history`; set `HISTFILE` to a different
value to change that or to an empty value to keep no file.

You can set the history options in `config.toml` (see [Settings in
config.toml](#settings-in-configtoml)), or with

```sh
setopt -p history \
    file=~/.histfile \
    save_size=10000 \
    share \
    ignore_space \
    reduce_blanks
```

The above example also demonstrates the `-p` option of `setopt` (for prefix),
which enables setting options in a group. It is equivalent to

```
setopt history.file=~/.histfile
setopt history.save_size=10000
setopt history.share
setopt history.ignore_space
setopt history.reduce_blanks
```


Unlike zsh, luish saves the history by default: zsh keeps no file unless
`HISTFILE` and `SAVEHIST` are set.

### History expansion

With `setopt history.expand` (zsh's `banghist` and bash's `histexpand` are
the same option), `!` refers to earlier commands, as in bash and zsh. It is
off by default, since `!` isn't special in POSIX sh, but the recommended
`config.toml` that luish offers on its first run turns it on.

```sh
$ echo one two three
one two three
$ sudo !!             # the previous command
$ ls !$               # its last word, three
$ echo !^ !:2 !*      # its first, second and all arguments
$ !ec                 # the newest command starting with ec
$ !?two?              # the newest command containing two
$ !-2                 # the command before the previous one
$ !42                 # command 42, as fc -l numbers them
$ ^three^four         # the previous command, with three replaced by four
```

A line with a reference is shown as expanded before it runs, and goes into
the history that way. Each line of a multi-line command is expanded as it is
read.

- **Events**: `!!`, `!n`, `!-n`, `!str`, `!?str?`, `!#` (the line typed so far),
  and `!{...}` around any of them to separate it from the text after it. A
  `!` alone before a word designator or modifier (`!$`, `!:2`) is the event
  of the reference before it in the line, or else the previous command, as
  in zsh (bash always uses the previous command).
- **Word designators**, after `:` (which can be left out before `^`, `$`, `*`,
  `%` and `-`): `n`, `^` (1), `$` (the last), `%` (the word `!?str?` found),
  `x-y`, `-y` (`0-y`), `x*` (`x-$`), `*` (`1-$`, empty if there are no
  arguments) and `x-` (`x*` without the last word). Words are split as the
  shell splits them, with operators (`|`, `&&`, `>`) as words of their own,
  and a range keeps the text between its words.
- **Modifiers**, each after a `:`: `h` (the directory: without the last `/`
  and what follows), `t` (the last component), `r` (without the
  extension), `e` (only the extension; `h`, `r` and `e` leave the text as it
  is when there is no `/` or extension), `a` (an absolute path) and `A`
  (also with symbolic links resolved; see
  [Parameter expansion](#parameter-expansion)), `s/old/new/` (the first `old`, or
  each with `gs`; `&` in `new` is `old`, an empty `old` is the previous one,
  and the last `/` can be left out at the end of the line), `&` (the
  previous substitution again; `g&` for each), `q` (quoted), `x` (each word
  quoted) and `p` (print the line and add it to the history, but don't run
  it).

A `!` doesn't start a reference in single quotes or `$'...'`, after a
backslash, in here-document bodies, comments or `$((...))`, before a blank,
`=`, `(`, `"` or an operator (so `echo hi!` and `[ a != b ]` work), nor in
`$!`, `${!name}` and `[!...]`. In double quotes it does. A reference that
can't be expanded is an error, and the line (with the lines before it of the
same command) is dropped without going into the history.

With `setopt history.verify` as well, the expanded line is put back in the
line editor instead of running, so that it can be checked or changed first
(Enter runs it). On terminals the editor doesn't support (`TERM=dumb`), it
runs at once.

## Tab completion

In an interactive shell, Tab starts completion.


- `$` and `${` complete variable names, and `${name[` the indices of the array (in numeric order) or the keys of an
  associative array, listed with their values;
- `cd`, `pushd` and `rmdir` complete directories; for `cd` and `pushd`, when none in the current directory match,
  the directories in `CDPATH` complete instead (listed with the `CDPATH` directory they are in), as in zsh;
- `export`, `local`, `readonly`, `unset`, `read` (except the prompt after `-p`), `getopts` (after the option
  string) and `for` (then `in`) complete variable names (`unset -f` completes function names);
- `alias` and `unalias` complete aliases;
- `type`, `hash` and `which` complete command names, and `help` completes built-ins;
- `fg`, `bg`, `jobs`, `wait` and `kill` complete job specs such as `%1`, listed with their commands (after `%` and
  a letter, they complete the command names instead, such as `%vim`);
- `kill -` and `kill -s` complete signal names, as do the arguments of `trap` after its action;
- `setopt` completes the options that are off and `unsetopt` those that are on,
  as in zsh.
- `plugin load` completes the plugins in the plugin directory, and `plugin unload` the loaded ones.

Plugins, through their extensions, can provide completion for other commands
(see [Completing a command's arguments](extensions.md#completing-a-commands-arguments)). Aliases are followed: if `g` is an alias for
`git`, then `g ` completes as `git ` does.

As in zsh, a word with a glob, a `$` or a command substitution in it is expanded instead: `ls *.md` Tab becomes
`ls a.md b.md c\ d.md ` (quoted, and followed by a space when there are several words), and `echo $HOME` Tab becomes
`echo /home/me`. A glob that matches nothing, or a variable that is empty or unset, is completed as usual (so
`$HO` Tab still completes variable names).

### The completion menu

The menu shows the matches in columns, or one per line with their descriptions
(such as the commands of jobs for `fg`). If it doesn't fit on the screen, it
scrolls, and its last line says which rows are shown. The next Tab selects the
first match and puts it in the line, and then:

| Key | Action |
|---|---|
| Tab, Shift-Tab | Select the next or the previous match |
| Arrow keys, Ctrl-N, Ctrl-P, Ctrl-F, Ctrl-B | Move down, up, right or left in the menu |
| Page Down, Page Up | Move a screenful down or up |
| Enter | Keep the match and close the menu |
| Esc, Ctrl-G | Put back the text typed and close the menu |

Any other key keeps the match, closes the menu and does what it usually does,
so you can type on after it. Before a match is selected, Down, Ctrl-N and
Shift-Tab also start selecting (Shift-Tab from the last match), but the other
keys do what they usually do: Enter runs the command, and Up goes back in the
history.


## Settings in `config.toml`

luish's settings can also be set in `~/.config/luish/config.toml` (or
`$XDG_CONFIG_HOME/luish/config.toml`), a [TOML](https://toml.io) file.


```toml
[options]
autosuggest = true

[options.history]
file = "~/.histfile"
save_size = 10000
share = true
ignore_space = true

[options.glob]
star = true
```

The values have TOML's types: `true` or `false` for an option, an integer for a
number, and a string for text, where a leading `~` is expanded to the home
directory (nothing else in it is expanded).

Aliases go in the `alias` table, each as `NAME = "VALUE"`, the same as `alias
NAME=VALUE`. Global and suffix aliases (`alias -g` and `alias -s`, see `help
alias`) go in its `global` and `suffix` tables:

```toml
[alias]
ll = "ls -l"
".." = "cd .."            # a name with other characters than letters, digits, _ and - is quoted

[alias.global]
G = "| grep"              # ls G foo runs ls | grep foo
"..." = "../.."

[alias.suffix]
pdf = "evince"            # notes.pdf runs evince notes.pdf
```

A string named `global` or `suffix` in `[alias]` is an ordinary alias with that
name (but TOML doesn't allow it in the same file as the table of that name).
Values are used as they are, with no `~` expansion.

Key bindings can be set in the `bindkey` table, each as `KEY = "WIDGET"`, the
same as `bindkey KEY WIDGET` (see `help bindkey`):

```toml
[bindkey]
Up = "up-line-or-history"
Down = "down-line-or-history"
"Ctrl-X Ctrl-E" = "undo"
"^[[1;5C" = "forward-word"   # Ctrl-Right, as zsh writes it
```

Variables go in the `env` table, which exports them, and in its `interactive`
table those that only interactive shells should set; the `vars` table sets
shell variables that aren't exported (for prompts and plugins, say). The
`path` table adds directories to `PATH`, in the order given, `before` or
`after` those it has:

```toml
[env]                     # interactive and login shells
EDITOR = "nvim"
GOPATH = "~/go"

[env.interactive]         # interactive shells only
LESS = "-R"

[vars]                    # interactive shells, not exported
WORDCHARS = "*?_-."

[path]
before = ["~/bin", "~/.cargo/bin"]
after = ["/opt/tools/bin"]
```

Values are strings (or integers), where only a leading `~` is expanded.
Interactive shells apply all four tables; login shells that aren't
interactive (which read nothing else of `config.toml`) apply `env`, without
`env.interactive`, and `path`. Scripts and `luish -c` read none of them, but
inherit what their shell exported. `path` applies after the variables, so it
adds to a `PATH` set in `[env]`. A directory that `PATH` has already is
skipped, wherever it is, so a shell started from another doesn't add it
again, and one started by `pixi shell`, `nix-shell` or a Python virtual
environment's activation keeps that environment's directories first.

Colour schemes go in the `colorscheme` table, and styles and the scheme to use
in the `style` table: see [Colour schemes](#colour-schemes).

A key that isn't a setting, a value of the wrong type, an alias name with `=`
in it, a variable name that isn't one, a key or widget that `bindkey`
doesn't take, or a style name or value that `style` doesn't take is reported
with its line and skipped; a file that isn't valid
TOML is reported and ignored.

You also [install plugins in
config.toml](plugins.md#installing-plugins-with-configtoml)).

To keep the same settings on several machines, put them in a plugin of your own
(see [](personal-plugin.md)) and share that
across them with git or another tool.

## Cached startup files

luish caches the effects of your startup files, so that a new shell can restore
their result instead of running them, which is much faster when they run slow
commands. The files go in two directories, each used only if it exists
(they work like zsh's `.zshrc` and `.zlogin`):

- `~/.config/luish/rc.d/` (or `$XDG_CONFIG_HOME/luish/rc.d/`): for every
  interactive shell, after `config.toml`. This is the place for what isn't
  inherited by the shells you start: aliases, functions and options. Scripts
  and `luish -c` don't read it.
- `~/.config/luish/login.d/`: for login shells, after `rc.d`. Its files replace
  `/etc/profile` and `~/.profile`. This is the place for the environment:
  exported variables such as `PATH`, which the programs and shells you start
  inherit.

In each directory, the files whose names end in `.lsh` run in byte order. luish
remembers what they did: the variables they set, exported or unset, and their
functions, aliases, options, traps and `umask`. An interactive shell then reads
`$ENV` and `luishrc`, which always run (they aren't in a directory), though
their `__luish_cache` blocks are cached, as below.

```sh
mkdir -p ~/.config/luish/login.d ~/.config/luish/rc.d
echo '. /etc/profile' > ~/.config/luish/login.d/00-system.lsh
echo 'export PATH=$HOME/bin:$PATH EDITOR=vim' > ~/.config/luish/login.d/10-env.lsh
echo "alias ll='ls -l'" > ~/.config/luish/rc.d/aliases.lsh
```

The caches are `~/.cache/luish/rc-HOST` and `~/.cache/luish/login-HOST` (or
under `$XDG_CACHE_HOME`). Each file has its own entry, which a new shell uses
as long as:

- the file, and the files it reads with `.`, are unchanged (luish compares
  their size and modification time);
- `PATH` and `HOME` have the values they had when the entry was built;
- the files before it in the directory did the same as then (and, for
  `rc.d`, `config.toml`, `plugins.lock` and the plugins it loads are
  unchanged);
- luish itself is the same build.

Otherwise the shell runs the file again, and the files after it, and saves
what they did. luish keeps a few entries for each file, so that shells started
with different values of `PATH` each find theirs: `PATH=$HOME/bin:$PATH` adds
to the `PATH` that the shell was started with.

The cache directory holds nothing that can't be rebuilt: it can be removed at
any time. luish marks it with a `CACHEDIR.TAG` file, so that backup tools that
honour the [convention](https://bford.info/cachedir/) skip it.

### Caching part of a file

A file with `__luish_cache` blocks isn't cached as a whole: it runs every
time, except for its blocks, each cached on its own, with what it depends on
listed:

```sh
# ~/.config/luish/rc.d/nvm.lsh
export NVM_DIR=$HOME/.nvm
__luish_cache env=(NVM_DIR) files=("$NVM_DIR/alias/default") {
    . "$NVM_DIR/nvm.sh"
}
export GPG_TTY=$(tty)
```

A block's entry is used as long as:

- the variables of `env=(...)` have the values they had when it was built
  (unset is not the same as empty);
- the files of `files=(...)` are unchanged. The words are expanded (`~`,
  variables, patterns) at every start, so command substitution isn't allowed
  there. A file that doesn't exist is unchanged until it is created, and a
  directory changes when files are added to it or removed from it;
- the files it reads with `.` are unchanged, and its text is the same
  (comments and layout don't count).

Both lists are optional: `__luish_cache { ... }` is cached until its text or
the files it reads change. Unlike a file's entry, a block's doesn't depend on
`PATH`, nor on what ran before it: only on what it lists. Its exit status is
saved too, but what it prints is shown only when it runs. Blocks are cached in
the files of `rc.d` and `login.d`, and in `$ENV` and `luishrc` (and the
plugins they load with `plugin load`), each in a cache of its own since
neither file is cached as a whole; elsewhere (in a script), and inside
another block, the body just runs.

A plugin can have blocks too. In a plugin that `config.toml` enables, or that
a file of `rc.d` loads with `plugin load`, a block is cached on its own, and
the entry that loads the plugin (that of `config.toml`, or of the file) also
depends on what the block lists: when it changes, the entry runs again, but
the plugin's block only if its own key changed. A block that ends with
`return` (or fails) isn't saved, nor is the entry it ran in, so the next shell
tries again.

For code that should never be cached, use `_uncached.lsh`, which runs every
time, after the other files of its directory:

- anything with side effects, such as starting `ssh-agent` or printing a message;
- values that differ between shells, such as `GPG_TTY=$(tty)`;
- anything that depends on the environment the shell was started in, such as
  `$SSH_CONNECTION` or `$DISPLAY`.

luish can't see what a command reads (an environment variable, a file), so
the keys are for you to get right: a file's entry depends only on `PATH` and
`HOME`, and a block's only on what it lists. `__luish_internal check-cache`
finds what they miss.

### Checking the caches

`__luish_internal check-cache` finds the changes that luish can't see: it
runs the startup files again, as a new shell would without the cache, and
compares the result with what the cache restores. It checks the caches of
this host that exist, or those named (`rc`, `login`, `startup`):

```text
$ __luish_internal check-cache
rc: /home/me/.cache/luish/rc-myhost
  generated Mon 28 Sep 2026 09:12:03 CEST (2 hours ago)
  up to date
login: /home/me/.cache/luish/login-myhost
  generated Fri 25 Sep 2026 18:40:51 CEST (2 days ago)
  last checked Mon 28 Sep 2026 08:00:02 CEST (3 hours ago)
  file changed: /home/me/.config/luish/shared/env.sh
  10-env.lsh: alias ll: added
  block at /home/me/.config/luish/login.d/20-nvm.lsh:2: variable NVM_BIN: changed
  rebuilt
```

For each cache, it shows when it was last built and, if it was checked since,
when it was last found up to date (in local time, in the format of the
locale), then what differs: files changed, added or removed, a different
build of luish, and, for each entry, the variables, functions, aliases,
options and so on that it restores differently. An entry is shown by its file
(`config.toml` for `config.toml` and its plugins, `post-rc.lsh` for the
plugins' `post-rc.lsh`), and a block by its file and line. A cache that is up
to date is kept and touched (its modification time is when it was last
checked); one that differs is rebuilt, so that the next shell uses the new
entries. Shells already running keep what they started with.

The files run in a new shell, started as the last one that built an entry of
the cache: with the same options (interactive, login) and the environment it
started with, which the cache records for this. So a check gives the same
result wherever it runs (another directory, a shell whose `PATH` the files
already changed, or `cron`). It rebuilds every entry it reaches, and keeps
those for other keys. Their output is discarded, and `_uncached.lsh` doesn't
run.

With `-q` (or `--quiet`), it prints only the reports of the caches it
rebuilt. The exit status is 0 if every cache was up to date, 1 if any was
rebuilt, and 2 on errors, so it can run from `cron` or a systemd timer:

```sh
__luish_internal check-cache -q || echo 'startup cache rebuilt'
```

The cache files hold the environment of the shell that last built them (mode
0600, as they already hold the variables the files export).


## Internals

luish's own commands are subcommands of the `__luish_internal` built-in.

### Saving and restoring the shell's state

`__luish_internal savestate` prints shell commands that recreate the current
state of the shell: the working directory, the file mode mask (`umask`),
variables (with their `export` and `readonly` attributes), traps, functions,
aliases and options. Reading them back with `.` restores that state, in the
same shell or in another:

```sh
__luish_internal savestate > ~/saved.sh
luish -c '. ~/saved.sh; myfunction'
```

Restoring adds to the current state: variables, functions and aliases defined since are kept. A variable that is
already `readonly` can't be restored, so reading the state back into the shell that saved it fails if it has any.
In `eval "$(__luish_internal savestate)"`, traps are lost, because a command substitution resets them.

### The version of luish

`__luish_internal print-git-rev` prints the git revision luish was built from, and `__luish_internal
print-git-rev-short` the same with the abbreviated hash. A build from sources with uncommitted changes adds `-dirty`,
and a build outside a git checkout prints `unknown`.
