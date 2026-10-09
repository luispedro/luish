# __luish_internal vared without a terminal (the cases run without one):
# usage errors and errors about the variable come first, then the one about
# the terminal. The variable is left as it was. `vared` itself is a
# built-in only in interactive shells.
err() { sed -n 's/^.*: \(vared: \)/\1/p'; }
v() {
    __luish_internal vared "$@" 2>&1 | err
    __luish_internal vared "$@" 2>/dev/null
    echo "status $?"
}
v
v a b
v -aA x
v -z x
v -p
v -t /dev/tty x
v -i w x
v -M vicmd x
v -m emacs x
v 1x
readonly r=1
v r
v nosuch
v 'nosuch[1]'
x=1
v x
echo "x=$x"
v -c new
echo "new=${new-unset}"
a=(p q)
v -A a
v 'a[1]'
typeset -p a
command -v vared; echo "status $?"
$SH -ic 'command -v vared' 2>/dev/null
