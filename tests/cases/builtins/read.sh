printf 'a b c\n' | { read x y; echo "[$x][$y]"; }
printf '  lead  trail  \n' | { read x; echo "[$x]"; }
printf 'a\\ b c\n' | { read x y; echo "[$x][$y]"; }
printf 'a\\ b c\n' | { read -r x y; echo "[$x][$y]"; }
printf 'line1\\\nline2\n' | { read x; echo "[$x]"; }
printf 'no newline' | { read x; echo "$? [$x]"; }
printf 'a:b:c\n' | { IFS=: read x y; echo "[$x][$y]"; }
printf 'one\ntwo\n' | { read a; read b; echo "$b $a"; }
printf 'x y\n' | { read a b c; echo "[$a][$b][$c]"; }
