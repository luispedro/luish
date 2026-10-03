# Language features compared

This page compares what the shell language of dash, bash, zsh and luish can do beyond the basics that all four share:
quoting, parameter expansion, arrays, arithmetic, conditionals, globbing, redirections, control flow and the
variables the shell sets. [](builtin-commands.md) compares the built-in commands.

Each feature was tried in dash 0.5.12, bash 5.2, zsh 5.9 (started with `-f`) and luish 0.3.0. The zsh column is
native zsh. Where luish takes a feature from zsh, it usually follows zsh's `sh` emulation (`zsh --emulate sh`), which
turns some of zsh's features off and counts array indices from 0; the notes say where that matters.

In the tables, ✓ marks a feature that works, – one that doesn't, and *opt* one that needs an option turned on.

luish turns on everything that gives a meaning to what is a syntax error in dash, such as `a=(x y)` or `[[`, so a
script can use it without any setting. What would change the behaviour of a POSIX script, such as `**/` or glob
qualifiers, needs an option, as in bash and zsh.

## Quoting

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `'...'`, `"..."`, `\` | ✓ | ✓ | ✓ | ✓ | |
| `$'a\tb'` | – | ✓ | ✓ | – | In POSIX since 2024 |
| `$"..."` (translated) | – | ✓ | – | – | |

## Parameter expansion

All four have POSIX's forms: `${x:-word}`, `${x:=word}`, `${x:?word}`, `${x:+word}`, `${#x}`, `${x#pattern}`,
`${x##pattern}`, `${x%pattern}` and `${x%%pattern}`.

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| Substrings, `${x:1:3}`, `${x: -3}` | – | ✓ | ✓ | ✓ | |
| Replacement, `${x/pat/rep}`, `${x//pat/rep}` | – | ✓ | ✓ | ✓ | `&` in the replacement is literal, as in zsh (bash 5.2 replaces it with the match) |
| Anchored replacement, `${x/#pat/rep}`, `${x/%pat/rep}` | – | ✓ | ✓ | ✓ | |
| Case, `${x^^}`, `${x^}`, `${x,,}` | – | ✓ | – | – | zsh and luish: `${(U)x}`, `${(L)x}`, `${(C)x}` |
| Transformations, `${x@Q}`, `${x@U}` ... | – | ✓ | – | – | |
| Indirection, `${!x}` | – | ✓ | – | ✓ | zsh: `${(P)x}` |
| Names by prefix, `${!prefix*}`, `${!prefix@}` | – | ✓ | – | ✓ | zsh: `${(k)parameters[(I)prefix*]}` |
| zsh's flags `@ k v j s f F L U C u o O i n a`, as in `${(j:,:)a[@]}`, `${(o)a[@]}`, `${(s:,:)x}`, `${(f)x}` | – | – | ✓ | ✓ | |
| zsh's other flags, such as `P`, `q`, `e`, `l:n:`, `r:n:`, `%` | – | – | ✓ | – | |
| Nested, `${${x#a}%b}` | – | – | ✓ | – | |
| Modifiers, `${x:h}`, `${x:t}`, `${x:r}`, `${x:e}`, `${x:A}` | – | – | ✓ | ✓ | luish has `h`, `t`, `r`, `e`, `a`, `A`, `u` and `l` |
| Lengths and offsets in characters | – | ✓ | ✓ | – | luish, as dash, counts bytes: in a UTF-8 locale, `x=é; echo ${#x}` prints 2 (bash and zsh: 1) |

## Arrays

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `a=(x y z)`, `"${a[@]}"`, `${#a[@]}`, `a+=(w)` | – | ✓ | ✓ | ✓ | |
| First index | | 0 | 1 | 0 | zsh's `sh` emulation counts from 0, as bash and luish |
| `$a` | | First element | All elements | First element | |
| `a[i]=x`, `${a[i]}`, `${a[-1]}` | – | ✓ | ✓ | ✓ | |
| Holes: `a=(); a[5]=x; echo ${#a[@]}` | – | 1 | 5 | 6 | bash's arrays are sparse; zsh and luish fill the gap with empty elements |
| Indices, `${!a[@]}` | – | ✓ | – | ✓ | |
| Slices, `${a[@]:1:2}` | – | ✓ | ✓ | ✓ | |
| zsh's ranges, `$a[2,3]` | – | – | ✓ | – | |
| On each element, `${a[@]/x/y}`, `${a[@]#p}` | – | ✓ | ✓ | ✓ | |
| `local a=(x y)`, `typeset -a` | – | ✓ | ✓ | ✓ | |
| Read into an array, `read -a` (bash), `read -A` (zsh) | – | ✓ | ✓ | ✓ | luish takes both |
| `mapfile`, `readarray` | – | ✓ | – | – | |
| Exported arrays | – | – | – | – | None of them export arrays |

### Associative arrays

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `typeset -A h`, `h[k]=v`, `${h[k]}`, `${#h[@]}` | – | ✓ | ✓ | ✓ | |
| `h=([k]=v [j]=w)` | – | ✓ | ✓ | ✓ | |
| `h=(k v j w)` | – | ✓ | ✓ | ✓ | |
| Keys, `${!h[@]}` | – | ✓ | – | ✓ | |
| Keys, `${(k)h[@]}` | – | – | ✓ | ✓ | |
| Order of the keys | | Hash order | Hash order | Insertion order | |

## Arithmetic

All four have `$((...))` with C's integer operators, assignments (`x+=2`), `?:`, hexadecimal (`0x10`) and octal
(`010`) numbers.

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `let` | – | ✓ | ✓ | ✓ | |
| `((...))` as a command | – | ✓ | ✓ | – | luish: `let '...'`, or `[ $((...)) -ne 0 ]` |
| `**` | – | ✓ | ✓ | – | |
| `++`, `--` | – | ✓ | ✓ | – | luish: `x=$((x + 1))` |
| `,` | – | ✓ | ✓ | – | |
| Bases, `16#ff` | – | ✓ | ✓ | – | |
| Floating point | – | – | ✓ | – | |
| Integer variables, `typeset -i` | – | ✓ | ✓ | ✓ | |

## Conditionals

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `[ ... ]`, `test` | ✓ | ✓ | ✓ | ✓ | |
| `[[ ... ]]`, with patterns (`[[ $x == *.c ]]`) | – | ✓ | ✓ | ✓ | |
| Regular expressions, `[[ $x =~ re ]]` | – | ✓ | ✓ | ✓ | luish sets both bash's `BASH_REMATCH` and zsh's `MATCH` and `match` |
| `[[ -v name ]]` | – | ✓ | ✓ | ✓ | |
| `[ -v name ]` | – | ✓ | ✓ | – | |

## Brace expansion

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `a{b,c}` | – | ✓ | ✓ | *opt* | luish: `setopt expand.braces`; not in zsh's `sh` emulation |
| `{1..10}`, `{a..z}`, `{01..10..2}` | – | ✓ | ✓ | *opt* | |
| `{1..$n}` | – | – | ✓ | *opt* | bash expands braces before variables |

Where bash and zsh differ (the sign of a step, `{a..e..2}`, `{1..a}`, empty words such as `{a,}`, `> f{1,2}`), luish
follows bash (see [](../compatibility.md)).

## Globbing

All four have POSIX's `*`, `?` and `[...]`. luish sorts matches by bytes, where bash and zsh use the locale's collation
(the same in the `C` locale).

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `**/`, any number of directories | – | *opt* | ✓ | *opt* | bash: `shopt -s globstar`; luish: `setopt glob.star` |
| Glob qualifiers, `*(/)`, `*(.om[0])` | – | – | ✓ | *opt* | luish: `setopt glob.bare_qualifiers`; most of zsh's qualifiers (see [](../globbing.md)) |
| ksh's patterns, `@(a\|b)`, `!(x)` | – | *opt* | *opt* | – | bash: `shopt -s extglob`; zsh: `setopt ksh_glob` |
| zsh's patterns, `(a\|b)`, `^x`, `x~y`, `x#` | – | – | ✓ | – | `^`, `~` and `#` need `setopt extended_glob` |
| No match gives nothing (`nullglob`) | – | *opt* | *opt* | – | zsh also has a qualifier, `*(N)`, which luish has |
| Matching dot files (`dotglob`) | – | *opt* | *opt* | – | zsh's `*(D)` qualifier, which luish has |

## Redirections and pipes

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| POSIX's, including `>\|` and here-documents | ✓ | ✓ | ✓ | ✓ | |
| File descriptors above 9, `exec 20>f` | – | ✓ | – | ✓ | dash and zsh read `20` as a word |
| Here-strings, `<<< word` | – | ✓ | ✓ | ✓ | |
| `&>`, `&>>` (stdout and stderr) | – | ✓ | ✓ | – | luish, as dash and POSIX, reads `cmd &> f` as `cmd &` then `> f`; write `> f 2>&1` |
| `\|&` (pipe stdout and stderr) | – | ✓ | ✓ | – | `2>&1 \|` |
| Process substitution, `<(cmd)`, `>(cmd)` | – | ✓ | ✓ | ✓ | |
| zsh's `=(cmd)` (a temporary file) | – | – | ✓ | – | |
| Allocated descriptors, `exec {fd}>f` | – | ✓ | ✓ | – | |
| `$(<file)` | – | ✓ | ✓ | – | `$(cat file)` |
| Coprocesses, `coproc` | – | ✓ | ✓ | – | |

## Control flow and functions

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `function name { ... }` | – | ✓ | ✓ | ✓ | |
| `local` | ✓ | ✓ | ✓ | ✓ | |
| `case` fall-through, `;&` | – | ✓ | ✓ | – | In POSIX since 2024 |
| `case` continue, `;;&` (bash), `;\|` (zsh) | – | ✓ | ✓ | – | |
| `for ((i=0; i<n; i++))` | – | ✓ | ✓ | – | `i=0; while [ $i -lt $n ]; do ...; i=$((i+1)); done` |
| `select` | – | ✓ | ✓ | – | |
| `time` as a keyword (times pipelines and functions) | – | ✓ | ✓ | – | luish and dash run the `time` program |
| `repeat N`, `foreach`, short loops (`for i in a b; cmd`) | – | – | ✓ | – | |
| Anonymous functions, `() { ...; } args` | – | – | ✓ | – | |

## Errors and debugging

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `set -e`, `set -u`, `set -x` | ✓ | ✓ | ✓ | ✓ | luish's `set -x` quotes arguments as bash does |
| `set -o pipefail` | – | ✓ | ✓ | ✓ | In POSIX since 2024 |
| `$LINENO` | – | ✓ | ✓ | ✓ | |
| `ERR` trap (zsh also `ZERR`) | – | ✓ | ✓ | – | |
| `DEBUG` trap | – | ✓ | ✓ | – | |
| Call stack: `BASH_SOURCE`, `FUNCNAME`, `BASH_LINENO`, `caller` (bash) | – | ✓ | – | ✓ | zsh has `funcstack`, `funcfiletrace` and `%x` instead, outside its sh emulation |
| The call stack in error messages | – | – | – | ✓ | bash and zsh name the file (bash) or the function (zsh) of the error, without the calls that led there |
| Deep recursion | Limit for functions (Debian's), crashes otherwise | Crashes, unless `FUNCNEST` is set | Limit (`FUNCNEST`) | Error | luish stops at 1000 levels of function calls, and makes other nesting that would run out of stack an error |

## Variables the shell sets

| Variable | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `RANDOM`, `SECONDS` | – | ✓ | ✓ | ✓ | |
| `EPOCHSECONDS`, `EPOCHREALTIME` | – | ✓ | *opt* | ✓ | zsh: `zmodload zsh/datetime` |
| `UID`, `EUID`, `HISTCMD` | – | ✓ | ✓ | ✓ | |
| `PIPESTATUS` (bash) | – | ✓ | – | ✓ | |
| `pipestatus` (zsh) | – | – | ✓ | ✓ | |
| `path`, `dirstack`, tied to `PATH` and the directory stack | – | – | ✓ | ✓ | |
| `$_` | – | ✓ | ✓ | – | |
| `SHLVL` | – | ✓ | ✓ | ✓ | |

## Variable attributes

| Feature | dash | bash | zsh | luish | Notes |
|---|:-:|:-:|:-:|:-:|---|
| `export`, `readonly`, `local` | ✓ | ✓ | ✓ | ✓ | |
| `typeset`/`declare` `-i`, `-l`, `-u`, `-r`, `-x`, `-g` | – | ✓ | ✓ | ✓ | |
| `typeset -U` (unique elements, as for `path`) | – | – | ✓ | ✓ | |
| Namerefs, `typeset -n` | – | ✓ | – | – | |
| Floating point, `typeset -F`, `-E` | – | – | ✓ | – | |
| Padding, `typeset -L`, `-R`, `-Z` | – | – | ✓ | – | |
| Tied variables, `typeset -T` | – | – | ✓ | – | luish ties only `path` and `dirstack` |
