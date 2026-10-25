# `[[ ... ]]`

```text
[[ expression ]]
```

Evaluate a condition, without splitting or globbing.

`[[` is a reserved word, as in bash and zsh, not a command: the shell parses
the expression, so words aren't split into fields or globbed, `$x` needs no
quotes, and operators are recognized only if they are unquoted. Words are
expanded only as far as the expression is evaluated. The exit status is 0
if the expression is true and 1 if it is false. A malformed expression is
a syntax error when the line is read.

It has the operators of `test` (see `help test`), with these differences:

`expr1 && expr2`, `expr1 || expr2`, `! expr`, `( expr )`
: And; or; not; grouping, without quoting the parentheses.

`-a file`, `-o option`
: `file` exists; the option is on (`-o pipefail`). Both are unary, so use
  `&&` and `||` to combine expressions.

`-v name`
: The variable (or positional parameter, `-v 1`) is set. Also `-N file`:
  `file` was modified after it was last read.

`s == pattern`, `s != pattern`
: `s` matches the pattern; doesn't, as in `case`. `=` is the same as `==`.
  Quote the right side to compare strings: `[[ $a == "$b" ]]`.

`s =~ regex`
: `s` matches the extended regular expression (the C library's), anywhere
  in `s` unless anchored with `^` and `$`. After it, `(` and `|` are part
  of the word: `[[ $x =~ ^(a|b)$ ]]`. A match sets `MATCH` (the matched
  text), `MBEGIN` and `MEND` (its offsets, from 0), and for groups, the
  arrays `match`, `mbegin` and `mend` (from index 0), as in zsh, and
  `BASH_REMATCH` (the match, then the groups), as in bash. A failed match
  changes none of them, and a bad regular expression is reported and false.

`s1 < s2`, `s1 > s2`
: `s1` sorts before; after `s2`, comparing bytes. No quoting needed.

`n1 -eq n2`
: The arithmetic expressions are equal: `[[ i+1 -lt n ]]`. Also `-ne`,
  `-lt`, `-le`, `-gt` and `-ge`.

```sh
if [[ $name == j* && -d $dir ]]; then cd "$dir"; fi
[[ $line =~ ^([a-z]+)=(.*)$ ]] && echo "${match[0]} is ${match[1]}"
```

Unlike zsh, there are no extended patterns such as `(a|b)` on the right of
`==`.
