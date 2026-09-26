x=1 y=2; echo $x $y
x=3 sh -c 'echo $x'; echo $x
unset z; z=5 true; echo "z=$z"
f() { echo "in f: $v"; }
v=temp f; echo "after: $v"
a=$(echo sub) ; echo $a
b=$(false); echo $?
c=$(exit 3) d=x; echo $?
IFS=: read p q <<EOF
one:two
EOF
echo "$p $q"
