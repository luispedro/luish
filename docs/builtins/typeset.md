# `typeset`

```text
typeset [-agrx] [+rx] name[=value]... name=(value...)...
typeset -p [-arx] [name...]
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

`-g`
: In a function, change the variables outside it rather than make local
  ones.

`-p`
: Print the variables named, or all those that have the attributes given
  (all of them without other options), as `typeset` commands that can be
  read back. Without names or other options, `typeset` does the same.

`-r`
: Make the variables read-only (see `readonly`). A read-only variable can't
  lose the attribute, but a local one hides it until the function returns.

`-x`
: Export the variables (see `export`). `+x` stops exporting them.

```sh
f() {
    typeset -a files=(*.txt)
    typeset -g count=${#files[@]}
}
```
