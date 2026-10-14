# Each assignment before a command sees the ones before it, not those after
FOO=foo BAR="[$FOO][$BAZ]" BAZ=baz env | grep -E '^(FOO|BAR|BAZ)=' | sort
f() { echo "f: $BAR $X"; }
X=1 BAR="[$X]" f
X=2 BAR="[$X]" command env | grep '^BAR='
X=3 BAR="[$X]" eval 'echo "eval: $BAR"'
X=4 BAR="[$X]" read r <<EOF2
x
EOF2
echo "after: [$X] [$BAR]"
X=5 BAR="[$X]" : ; echo "special: $X $BAR"
