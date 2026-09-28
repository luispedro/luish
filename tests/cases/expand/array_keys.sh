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
# A lone "${!e[@]}" gives no field, as "$@" does.
set -- "${!e[@]}"
echo "$#"
# No operator (`${!a[1]}` is an indirection: expand/indirect.sh).
(echo "${!a[@]:-d}") 2>/dev/null
echo "status $?"
# `$!` still works.
echo "[${!}] [${!-unset}]"
f() { echo "${!a[@]}" ${!h[*]}; }
f
