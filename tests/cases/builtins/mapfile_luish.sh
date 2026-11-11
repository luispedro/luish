# `mapfile` where luish differs from bash.
show() { printf '<%s>' "$@"; echo; }
printf 'a\nb\nc\n' > f
# Arrays have no holes: -O past the end makes the elements before it empty
# (and the callback counts them).
x=(1 2); mapfile -t -O 4 x < f; show "$?" "${#x[@]}" "${x[@]}"
unset x; mapfile -t -O 2 -n 1 x < f; show "${#x[@]}" "${x[@]}"
cb() { show cb "$@" "${#x[@]}"; }
unset x; mapfile -t -O 2 -C cb -c 2 x < f; show "${x[@]}"
# With -O and nothing read, nothing is assigned.
unset x; mapfile -O 3 x < /dev/null; show "$?" "${#x[@]}" "${x-unset}"
mapfile -O 67108864 x < f; show "$?"
# NUL bytes are dropped, as `read` drops them (bash ends the line there).
printf 'a\0b\nc\0\n\0' | { mapfile -t x; show "${#x[@]}" "${x[@]}"; }
# A bad name has status 2, as for `read` (bash: 1).
mapfile -t 1x < f; show "$?"
mapfile -t 'x[1]' < f; show "$?"
mapfile -t '' < f; show "$?"
# A read-only array: an error before anything is read, which the shell
# survives.
readonly ro=(1)
{ mapfile -t ro; show "$?" "${ro[@]}"; read r; show "$r"; } < f
{ mapfile -t -O 1 ro; show "$?" "${ro[@]}"; } < f
# Tied arrays.
mapfile -t path <<E
/a
/b
E
show "$PATH"
