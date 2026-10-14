# Assignments whose values span lines, in a script read from standard input
printf '%s\n' "p='a" "b'" 'echo "[$p]"' "q=x\"y" "z\" r=1" 'echo "[$q] $r"' "s='a" "b' t=\$s" 'echo "[$t]"' \
    "u='a" "b' env | grep '^u='" "v=\$(echo a" "echo b)" 'echo "[$v]"' | $SH
