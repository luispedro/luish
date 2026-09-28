# Where luish's associative arrays differ from zsh's sh emulation.
# `$h` is `${h[0]}`, and `h=x` assigns to it, as in bash (zsh reads some
# value, and refuses the assignment).
typeset -A h
h[a]=1
echo "[$h] ${#h}"
h=x
h+=y
echo "$h ${h[0]} ${h[a]} ${#h[@]}"
# `typeset -A` makes a string the value at key 0, as in bash (zsh empties
# it), and an indexed array can't become an associative one or the other
# way around (bash; zsh empties it).
s=str
typeset -A s
echo "${s[0]}"
a=(1 2)
typeset -A a
echo "status $? ${a[1]}"
typeset -a h
echo "status $? ${h[a]}"
typeset -A t=v
echo "${t[0]}"
# `unset 'h[key]'` of a read-only one is an error, as in bash (zsh removes
# the key).
readonly t
(unset 't[0]'; echo never)
echo "status $?"
# `typeset -p` quotes the keys and the values, `set` too.
typeset -A p=([a]=1 ["b c"]="it's" [d]=)
typeset -p p
typeset -A e
typeset -p e
set | grep '^p='
# `typeset -A` without names prints the associative arrays.
typeset -A | grep -v '^typeset -Ar t='
# The state can be saved and restored.
__luish_internal savestate > state
$SH -c '. ./state; typeset -p p'
$SH -c 'f() { . ./state; }; f; typeset -p p'
# `set -x` shows the keys.
k='a b'
(
  set -x
  p[$k]=2
  p=([k]=v ["$k"]=w)
  p+=(x y)
) 2>&1
