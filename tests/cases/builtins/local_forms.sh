# The forms of local's arguments: plain values are expanded without
# building an assignment word, and tildes, ${x-word} and the rest as
# assignments.
HOME=/home/me
f() {
	local a=~ b=x:~/y c="$1" d=$1 e= f="" g=${u-~/x} h="${u-~}" i=$((1+2)) j=a"$1"b'c' k l=$u m=~me
	printf '%s|' "$a" "$b" "$c" "$d" "$e" "$f" "$g" "$h" "$i" "$j" "${k-unset}" "$l" "$m"
	echo
	local n
	n=inner
	echo "$n"
}
k=outer n=outer
f 'one two'
echo "$k $n"
# $@ and $* with operators.
g() {
	printf '<%s>' "$@" $@ "$*" $* ${@#a} "${*#a}" ${@-none} "${*:-none}" "${@+set}"
	echo
}
g ab 'c d' a
g
set -u
g
