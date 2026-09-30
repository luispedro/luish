# reference: zsh
# `print` (as in zsh) is a built-in in interactive shells. zsh here runs
# natively, where `print -P` expands `%` sequences.
$SH -i +m -c '
print a b c; print -n x; print y
print; print -l; print -ln; print -n; print -- -n x; print - -n x; print -123
print -l a b "c d"
print -N a b; print -l -N a b; print -nN a b; echo
print "a\tb\\\\c" x
print -r "a\tb\n"
print "\101\0101\x41\x4g\xg\e\E\q\"é\M-\C-a\C-?\Mx" | od -An -c
print "a\cb" c d; echo "|"
print -l x "a\cb" c; echo "|"
print -r "a\cb"
print -b "^a\cb" | od -An -c
print -R "a\tb" -n z; print -R -e "a\tb"; print -R -en "a\tb"; echo
print -R -r x; print -R -- -n x; print -R - x; print -rR "a\tb"
print -o c a B b; print -O c a b; print -oi c a B b; print -O -i a B b c; print -i c a B
print -o c "b\c" a; echo "|"
print -D "$HOME" "${HOME}x" "$HOME/a" /tmp; print -D -r "$HOME\t"
print -m "a*" abc bcd ab; print -m "?" -l a bb c; print -o -m "*" b a
print -f "%s-%d\n" a 1 b 2; print -f "[%s]" a b; echo; print -f "%s\n"; print -rf "%s\n" "a\tb"
print -f "%s" -v q a b; echo "[$q]"
print -v v -l x y; echo "[$v]"; print -v v x "a\cb" c; echo "[$v]"
print -v v -C 2 a b c; echo "[$v]"; v=1; print -v v; echo "[$v]"
print -v v a b; echo "[$v]"; print -v v -ln a b; echo "[$v]"; print -v v -l; echo "[$v]"
print -v v -lN a b; echo "[$v]" | od -An -c; print -v v -N a b; echo "[$v]" | od -An -c
print -u 2 to stderr 2>&1
print -C 2 1 2 3 4 5; print -a -C 2 1 2 3 4 5; print -aC2 1 2 3 4 5 | od -An -c
print -C 3 a bbbbbb c dd; print -aC 3 a bbbbbb c dd e; print -NC 2 a b c; echo; print -n -C 2 a b c
print -C 2 x "a\cb" c; echo "|"; print -c; print -C 2
COLUMNS=20; print -c a bb ccc dddd e f g; print -ac a bb ccc dddd e f g
print -x 4 "a" "\tb" "c\n\td"; print -X 4 "a" "\tb" "c\n\td"; print -X3 "ab\tc\td"
print -P "%%|%?|[%~]|[%1~]|%F{red}r%f|%{x%}|%(?.ok.bad)"
false; print -P "%?"
print -P "\x25?"; print -Pr "\t%%"; print -P -- "-%%"; print -Pn a; print -P b; print -lP "%%a" "%%b"
print -Pf "%s %%\n" "%%"
print -D -P "%/"
' </dev/null
echo "status $?"
# Errors.
for opts in -q -e -E -Rq -C -u -v -f -x -m '-C 0' '-C r' '-u x' '-u 9' '-x 0' '-x a' -p; do
  $SH -i +m -c "print $opts x; echo \$?" </dev/null 2>/dev/null
done
