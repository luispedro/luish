printf '%s-%s\n' a b c d
printf '%d %i %o %x %X %u\n' 42 -3 8 255 255 7
printf '%5s|%-5s|%.2s\n' ab cd efgh
printf '%05d|%+d|% d|%-4d|\n' 42 5 5 7
printf '%c%c\n' hello world
printf '%b\n' 'a\tb\c' 'x'
printf '%%\n'
printf '%s\n'
printf '%d\n' "'A" '"B'
printf 'no newline'
echo
printf '%.3f %e %g\n' 3.14159 12345.678 0.0001
printf '%*d|%-*d|\n' 5 1 4 2
printf '\101\102\n'
printf '%d\n' abc 2>/dev/null; echo "status $?"
