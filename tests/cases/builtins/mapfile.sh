# `mapfile` and `readarray` (the expected output is bash's).
show() { printf '<%s>' "$@"; echo; }
printf 'a\nb\nc\nd\n' > f
# Each line is an element, with its newline; MAPFILE without a name.
printf 'a\nb\nc\n' | { mapfile; show "$?" "${#MAPFILE[@]}" "${MAPFILE[@]}"; }
# -t removes the newline. A last line without one counts.
printf 'a\nb' | { mapfile -t x; show "$?" "${#x[@]}" "${x[@]}"; }
printf 'a\nb' | { mapfile x; show "${x[@]}"; }
printf '\n\n' | { mapfile -t x; show "${#x[@]}" "${x[@]}"; }
# Nothing is split or trimmed, and backslashes are ordinary.
printf '  a  b \\\n\\x\tc\\\\\n' | { IFS=a mapfile -t x; show "${#x[@]}" "${x[@]}"; }
printf 'a\r\nb\n' | { mapfile -t x; show "${#x[0]}"; }
# The array is replaced: also when nothing is read, and a string.
printf '' | { x=(1 2); mapfile -t x; show "$?" "${#x[@]}" "${x[@]}"; }
s=str; mapfile -t s < f; show "$?" "${s[@]}"
readarray -t x < f; show "$?" "${x[@]}"
mapfile -t x <<E
here $s
doc
E
show "${x[@]}"
mapfile -t x < <(echo one; echo two); show "${x[@]}"
# Names after the first are ignored.
mapfile -t x y z < f; show "$?" "${x[@]}" "${y-unset}"
mapfile -t -- x < f; show "$?" "${x[@]}"
# -n: at most so many lines, and the rest is left to read, from a pipe
# and from a file; 0 is all of them.
printf 'a\nb\nc\n' | { mapfile -t -n 2 x; read r; show "${x[@]}" "$r"; }
{ mapfile -t -n 2 x; read r; show "${x[@]}" "$r"; } < f
{ mapfile -tn2 x; mapfile -t y; show "${x[@]}" "${y[@]}"; } < f
mapfile -t -n 0 x < f; show "${x[@]}"
mapfile -t -n ' 1 ' x < f; show "$?" "${x[@]}"
mapfile -t -n +1 x < f; show "$?" "${x[@]}"
mapfile -t -n 9 x < f; show "$?" "${x[@]}"
# -s: lines skipped first, which -n doesn't count.
printf 'a\nb\nc\nd\n' | { mapfile -t -s 1 -n 2 x; read r; show "${x[@]}" "$r"; }
mapfile -t -s 3 x < f; show "${x[@]}"
mapfile -t -s 9 x < f; show "$?" "${#x[@]}"
# -O: from that index on, keeping the other elements (also with -O 0).
x=(1 2 3 4 5); mapfile -t -O 1 -n 2 x < f; show "$?" "${x[@]}"
x=(1 2); mapfile -t -O 1 x < f; show "${x[@]}"
x=(1 2); mapfile -t -O 2 x < f; show "${x[@]}"
x=(1 2); mapfile -t -O 0 x < /dev/null; show "${x[@]}"
x=(1 2 3); mapfile -t -O 0 -n 1 x < f; show "${x[@]}"
s=str; mapfile -t -O 1 s < f; show "${s[@]}"
unset x; mapfile -t -O 0 x < f; show "${x[@]}"
# -d: the first character of its argument ends the lines; '' is NUL.
printf 'a:b:c' | { mapfile -d : x; show "${x[@]}"; }
printf 'a:b\nc:' | { mapfile -t -d :x x; show "${#x[@]}" "${x[@]}"; }
printf 'a\0b c\0' | { mapfile -t -d '' x; show "${#x[@]}" "${x[@]}"; }
printf 'a\0b\n\0c' | { mapfile -d '' x; show "${#x[@]}" "${x[@]}"; }
# -u: another fd.
mapfile -t -u 3 x 3< f; show "${x[@]}"
mapfile -tu3 -n1 x 3< f < /dev/null; show "${x[@]}"
# -C: the callback runs every -c lines (5000 without it), with the index
# and the line (already without its newline, with -t), before the line is
# assigned; it finds the lines before it in the array.
cb() { show cb "$@" "${#x[@]}"; }
printf 'a\nb\nc\nd\ne\n' | { mapfile -t -C cb -c 2 x; show "${x[@]}"; }
printf 'a\nb\n' | { mapfile -C cb -c 1 x; show "${x[@]}"; }
printf "a'q \$s\nb\n" | { mapfile -t -C 'cb 1 "2 3"' -c 1 x; show "${x[@]}"; }
x=(1 2 3 4); mapfile -t -O 2 -C cb -c 1 x < f; show "${x[@]}"
mapfile -t -C cb x < f; show "${x[@]}"
mapfile -t -c 1 x < f; show "$?" "${x[@]}"
mapfile -t -s 1 -n 2 -C cb -c 1 x < f; show "${x[@]}"
# The callback may read from the input, a pipe or a file.
printf 'a\nb\nc\nd\n' | { mapfile -t -C 'read z; show z "$z"; cb' -c 1 x; show "${x[@]}"; }
mapfile -t -C 'read z; show z "$z"; cb' -c 1 x < f; show "${x[@]}"
# In functions, and with attributes.
fn() { local l; mapfile -t l < f; show "${l[@]}"; mapfile -t g < f; }
fn; show "${l-unset}" "${g[@]}"
declare -u up; printf 'ab\ncd\n' | { mapfile -t up; show "${up[@]}"; }
declare -i n; printf '1+1\n2*3\n' | { mapfile -t n; show "${n[@]}"; }
# More than one block is read at a time: lines that cross their ends.
i=0; while [ "$i" -lt 12000 ]; do echo "line $i of the big file"; i=$((i + 1)); done > big
mapfile -t x < big; show "${#x[@]}" "${x[0]}" "${x[6000]}" "${x[11999]}"
cat big | { mapfile x; show "${#x[@]}" "${x[0]}" "${x[6000]}" "${x[11999]}"; }
cat big | { mapfile -t -n 7000 -s 100 x; read r; show "${#x[@]}" "${x[0]}" "${x[6999]}" "$r"; }
{ mapfile -t -n 7000 -s 100 x; read r; show "${#x[@]}" "${x[0]}" "${x[6999]}" "$r"; } < big
mapfile -t -C 'read z; : ' -c 5000 x < big; show "${#x[@]}" "${x[4999]}" "${x[5000]}"
# Errors.
mapfile -t -u 7 x; show "$?"
mapfile -t -u x x; show "$?"
mapfile -t -n x x < /dev/null; show "$?"
mapfile -t -n -1 x < /dev/null; show "$?"
mapfile -t -s x x < /dev/null; show "$?"
mapfile -t -O x x < /dev/null; show "$?"
mapfile -t -O -1 x < /dev/null; show "$?"
mapfile -c 0 x < /dev/null; show "$?"
mapfile -c x x < /dev/null; show "$?"
mapfile -q x < /dev/null; show "$?"
mapfile -n; show "$?"
mapfile -t '' < /dev/null; show "$?"
declare -A h; mapfile -t h < f; show "$?" "${#h[@]}"
readonly ro=(1); { mapfile -t ro; read r; show "$?" "${ro[@]}" "$r"; } < f
