# `${a[i..j]}`: Python's slices, from 0, up to (not including) `j`, as a
# list like `${a[@]}`. No other shell has them.
pr() { printf '<%s>' "$@"; echo; }
a=(p q r s t)
pr "${a[1..3]}"
pr ${a[1..3]}
pr "x${a[1..3]}y"
# Missing ends are the start and the end.
pr "${a[..2]}" "${a[3..]}"
pr "${a[..]}"
# A negative end counts from the end, and the ends are clamped.
pr "${a[-2..]}" "${a[..-1]}" "${a[-1..-2]}"
pr "${a[-10..10]}"
# Empty if the start isn't before the end: no words in double quotes.
pr "${a[3..1]}" "${a[9..]}" "${a[2..2]}"
# The ends are arithmetic expressions, expanded first.
i=1 j=4
pr "${a[i..j]}" "${a[$i..$j]}" "${a[i+1..j-1]}" "${a[$(echo 3)..]}"
# The ends are evaluated before the array is read.
pr "${a[k=2..k+1]}" "$k"
# A quoted `..` isn't a slice.
(echo "${a['1..2']}") 2>/dev/null || echo "status $?"
(echo "${a[1\..2]}") 2>/dev/null || echo "status $?"
# Elements stay whole, as with "${a[@]}".
b=(x 'y z' '')
pr "${b[1..]}"
pr ${b[1..]}
# Operators apply as to "${a[@]}".
pr "${#a[1..3]}" "${b[..2]#?}" "${a[1..4]/r/R}" "${a[1..4]:1:2}"
pr "${a[3..3]:-empty}" "${a[1..2]:+set}" "${a[3..3]:+set}"
pr "${(j:,:)a[1..4]}" "${(O)a[1..4]}"
# A string is an array of one element, as for `${s[0]}`.
s=hello
pr "${s[0..]}" "${s[1..]}" "${s[-1..]}"
# Unset: no words, and an error with set -u, as for "${u[@]}".
pr "${u[0..2]}" x
(set -u; echo "${u[0..2]}") 2>/dev/null || echo "status $?"
# Of an associative array, the subscript is a key.
typeset -A h
h[a..b]=AB
h[..]=dots
k=a
pr "${h[a..b]}" "${h[..]}" "${h[$k..b]}" "${h[x..y]}" "${#h[a..b]}"
# Special arrays.
true | false | true
pr "${pipestatus[1..]}"
# Errors.
(echo "${a[1..2..3]}") 2>/dev/null || echo "status $?"
(echo "${!a[0..1]}") 2>/dev/null || echo "status $?"
# Unquoted outside braces, `[1..2]` is a pattern, as in dash.
echo $a[1..2]
f() { echo "${a[1..2]}" ${a[..-1]} "${#a[i..]}"; }
typeset -f f
