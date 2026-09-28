# Where luish's `typeset` differs from zsh's sh emulation.
# `typeset -p` prints commands that recreate the variables, quoted as
# `export -p` does (zsh quotes only where needed, and uses `export` for
# exported strings).
a=(x "y z" "it's") s="a b" e=()
readonly r=1
export ex=2
readonly ex
typeset u
typeset -p a s e r ex u
declare -p a
# Without names, `-p` prints the variables with the attributes given.
typeset -p -a
typeset -r
# `typeset -a` makes a string an array of its value, as in bash (zsh
# empties it, and refuses `typeset -a x=value`).
x=1
typeset -a x
typeset -a y=2
typeset -p x y
# `local` keeps the value, as in dash, also with `-a`.
f() {
  local -a s
  typeset -p s
}
f
# Errors.
typeset -z q
echo "status $?"
(typeset 1x=1; echo never)
echo "status $?"
(typeset +r r; echo never)
echo "status $?"
(typeset r=2; echo never)
echo "status $?"
typeset -p nosuch
echo "status $?"
f() { local -g x; }
(f; echo never)
echo "status $?"
