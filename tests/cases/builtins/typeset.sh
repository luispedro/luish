# reference: zsh
# `typeset` and `declare` (not POSIX; as in zsh and bash).
typeset x
echo "[${x+set}]"
typeset y=1 z="a b" y=2
echo "$y|$z"
w='c d'
declare v=$w u=*
echo "$v|$u"
# In a function, the variables are local, and start unset (unlike with
# `local`, which keeps the value, as in dash).
x=out
f() {
  typeset x
  echo "[${x-unset}]"
  x=in
  typeset x
  echo "$x"
  typeset -g g=global
  typeset g=local
  echo "$g"
}
f
echo "$x $g"
# Arrays.
typeset -a a=(1 "2 3") b
echo "${#a[@]} ${a[1]} ${#b[@]}"
f() {
  typeset -a a
  a+=(q)
  echo "${a[@]}"
}
f
echo "${a[@]}"
# Attributes.
typeset -x e=1
sh -c 'echo "[$e]"'
typeset +x e
sh -c 'echo "[$e]"'
f() {
  typeset -r r=1
  typeset -x e=2
  sh -c 'echo "[$e]"'
  typeset -gx ge=3
}
f
r=2
echo "$r"
sh -c 'echo "[$e] [$ge]"'
# A local hides a readonly variable.
readonly ro=1
f() {
  typeset ro=2
  echo "$ro"
}
f
echo "$ro"
# A local isn't exported, even if the variable it hides is.
export x=1
f() {
  typeset x=2
  sh -c 'echo "[$x]"'
}
f
typeset -p nosuch 2>/dev/null
echo "status $?"
# `local` takes the same options.
f() {
  local -a la=(1 2)
  local -x lx=3
  echo "${la[1]}"
  sh -c 'echo "$lx"'
}
f
echo "[${la-unset}]"
