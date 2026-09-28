# reference: zsh
# ${x/pattern/replacement}, ${x//...}, ${x/#...} and ${x/%...}.
x=a.b.c
echo ${x/./-} ${x//./-} ${x/#a/X} ${x/%c/X} ${x/./} ${x/.} ${x//}
x=abc
echo ${x/b*/Z} "${x//[ac]/_}" ${x/#/P} ${x/%/S} ${x/#b/P} ${x/%b/S}
echo ${x//b*/Q} ${x/%*c/Q} ${x/#*/Q} ${x//*/Q} ${x/d/Q}
x=aaa
echo ${x//a*/b} ${x//*/b} ${x/a?/b} ${x//a/bb}
x=abcabc
echo ${x/%b*/Q} ${x/#*b/Q} ${x//b/} ${x/b}
# Quoting in the pattern and the replacement.
x=abc
y=b
p='?'
echo ${x/$y/Q} ${x/"*"/Q} ${x/$p/Q} ${x/"$p"/Q} ${x//?/<&>}
x='*'
echo ${x/\*/Q} ${x/"*"/Q}
x=abc
echo "${x/b/"*"}" ${x/b/\/} "${x/b/\}}" ${x/b/'q'} ${x//b/x/y}
x=a/b
echo ${x/\//:} ${x//\//:} "${x/\//:}"
HOME=/home/me
echo ${x/b/$HOME} "${x/b/~}" ${x/b/~}
x='a b c'
echo ${x// /:} ${x/ /x}
printf '<%s>' ${x/a/1 2} "${x/a/1 2}"
echo
# Unset and empty.
unset u
e=
echo "[${u/a/b}] [${e/a/b}] [${u/*/Q}] [${e/*/Q}] [${u/#/P}]"
# The positional parameters: each of them.
set -- ab cb 'a b'
echo ${@/b/Z} ${*/#/-}
printf '<%s>' "${@/a/x}" "${*/c/x}"
echo
IFS=:
echo "${*/a/c}"
IFS=' '
set --
for a in "${@/a/b}"; do echo never; done
x=abc
x=${x/b/B}
echo $x
# In double quotes, "${*/...}" replaces in the joined string, as in zsh.
set -- abc dbe
echo "${*/b/X}" "${*//b/X}" "${@/b/X}" ${*/b/X}
x=${*/b/X}
echo "$x"
cat <<E
${*/b/X}
E
