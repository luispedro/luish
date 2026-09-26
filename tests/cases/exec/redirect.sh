echo out > f1; cat f1
echo more >> f1; cat f1
cat < f1 | wc -l
echo err 2>&1 >/dev/null | cat
{ echo e1 >&2; } 2>f2; cat f2
exec 3>f3; echo via3 >&3; exec 3>&-; cat f3
exec 4<f1; read line <&4; echo "$line"; exec 4<&-
echo x >f4 2>&1; cat f4
: > f5; test -s f5 || echo empty
echo rw 1<>f6; cat f6
cat <nonexistent; echo "status $?"
echo abc > f7; set -C; echo def > f7; echo "clobber $?"; echo ghi >| f7; cat f7; set +C
echo dup >&-; echo "closed $?"
{ echo a; echo b >&2; } > f8 2>&1; cat f8
