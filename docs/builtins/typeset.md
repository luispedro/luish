# `typeset`

```text
typeset [-aAgilruUx] [+ilruUx] name[=value]... name=(value...)...
typeset -p [-aAilruUx] [name...]
typeset -f|+f [name...]
declare ...
```

Declare variables and set their attributes.

Not POSIX; as in zsh and bash. Each variable is set to `value`, if one is
given, and gets the attributes given as options (`+` removes one). In a
function, the variables are local to it (unless `-g` is given), and start
unset, unlike with `local`, which keeps the value. Outside a function,
`typeset name` declares `name` without setting it. Arguments that look like
assignments are expanded as assignments are, as for `export`. `declare` is
another name for `typeset`.

`-a`
: Make the variables arrays: a string becomes an array of one element, and
  a variable that isn't set becomes an empty array.

`-A`
: Make the variables associative arrays: a string becomes the value at key
  `0`, and a variable that isn't set becomes an empty associative array.
  Their values are given as `name=([key]=value...)`, or as pairs of keys
  and values, `name=(key value...)`. An array can't become an associative
  one, nor the other way around.

`-f`
: Print the definitions of the functions named (all of them without
  names), as text that can be read back. `+f` prints only their names. The
  status is 1 if a name isn't a function. It takes no variable attributes.

`-g`
: In a function, change the variables outside it rather than make local
  ones.

`-i`
: Give the variables the integer attribute: each value assigned to them
  (also by `read`, `for` or `local`) is evaluated as an arithmetic
  expression, and stored in decimal, and `name+=expr` adds. A value they
  already have is evaluated too. For an array, each element is evaluated.
  An error in the expression is an error of the assignment. The attribute
  is removed by `+i` and `unset`, and doesn't apply to a local made by
  `local` or to an assignment before a command (`x=1+1 cmd`).

`-l`
: Convert the values assigned to the variables (each element of an array)
  to lower case, as `-i` evaluates them: ASCII letters only. `-l` and `-u`
  replace each other.

`-p`
: Print the variables named, or all those that have the attributes given
  (all of them without other options), as `typeset` commands that can be
  read back. Without names or other options, `typeset` does the same.
  Special parameters such as `path`, `pipestatus` and `RANDOM` are printed
  with their current value when named, but listed only if they have
  attributes.

`-r`
: Make the variables read-only (see `readonly`). A read-only variable can't
  lose the attribute, but a local one hides it until the function returns.

`-u`
: Convert the values assigned to upper case, as `-l` does to lower case.

`-U`
: Keep only the first of equal elements in arrays (zsh), whenever they are
  assigned. On `path`, this removes repeated directories from `PATH` (so
  does `-U` on `PATH` when it is assigned).

`-x`
: Export the variables (see `export`). `+x` stops exporting them.

```sh
f() {
    typeset -a files=(*.txt)
    typeset -gi count=${#files[@]}
    count+=1
    typeset -A seen=([.]=1 [..]=1)
}
typeset -U path
path=(~/bin "${path[@]}")
```
