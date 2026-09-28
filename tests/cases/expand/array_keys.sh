# `${!a[@]}` and `${!a[*]}`: the indices of an array, or the keys of an
# associative one, as in bash (zsh's sh emulation doesn't have them).
a=(x y z)
echo "${!a[@]}"
for i in "${!a[@]}"; do echo "$i=${a[i]}"; done
IFS=:
echo "${!a[*]}" ${!a[*]}
unset IFS
# A string has index 0; an empty or unset array has none, even with set -u.
s=abc
echo "${!s[@]}"
e=()
echo "[${!e[@]}] [${!nosuch[*]}]"
(set -u; echo "[${!nosuch[@]}]")
# Keys come in the order they were added (bash uses its hash order).
typeset -A h
h=([k]=1 ["a b"]=2)
h[z]=3
printf '<%s>' "${!h[@]}"; echo
printf '<%s>' ${!h[@]}; echo
printf '<%s>' "${!h[*]}"; echo
unset 'h[k]'
printf '<%s>' "${!h[@]}"; echo
# The index must be @ or *, with no operator (bash's indirection, `${!x}`,
# isn't supported).
(echo "${!a[1]}")
echo "status $?"
(echo "${!a[@]:-d}")
echo "status $?"
(x=a; echo "${!x}")
echo "status $?"
# `$!` still works.
echo "[${!}] [${!-unset}]"
f() { echo "${!a[@]}" ${!h[*]}; }
f
