# reference: zsh
# ${x:offset} and ${x:offset:length}, with zsh's rules for negative values.
x=abcdef
echo ${x:1} ${x:1:2} ${x:0} ${x:2:0} ${x:10} ${x:6}
echo ${x: -2} ${x:(-2)} ${x: -3:2} ${x: -6:1} ${x: -7:1} ${x: -10}
echo ${x:1:-1} ${x:0:-6} ${x:2:10}
echo ${x:-1} ${x:+1} ${x:~1} ${x:!0}
i=1
echo ${x:1+1:2} ${x:$i*2} ${x:$((i)):1} ${x: 1 + 1 : 1 } ${x:1:${#x}}
echo "${x:1:2}${x: -1}" "${x:1:2} ${x:3}"
y="a  b"
printf '<%s>' ${y:0} "${y:0}" ${y:1}
echo
unset u
e=
echo "[${u:1}] [${e:1}] [${u:1:2}]"
# The positional parameters: offset 0 is $0, and a negative offset counts
# from the end.
set -- a "b c" d
echo ${@:2} ${@:2:1} ${@:4} ${@:5} ${@:1:0}
echo ${@: -1} ${@: -2:1} ${@: -5} ${@:1:-1}
printf '<%s>' "${@:2}" "${*:2}" ${@:2}
echo
IFS=:
echo "${*:1:2}"
IFS=' '
echo ${1:1} ${2:1:1}
set --
for a in "${@:1}"; do echo never; done
printf '<%s>' "${*:1}"
echo
f() { echo "${x:2:2}"; }
f
x=${x:3}
echo $x
