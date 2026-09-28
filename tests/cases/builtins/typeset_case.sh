# reference: zsh
# typeset -l and -u: values are converted to lower or upper case.
typeset -l x=ABC
echo "$x"
x=DeF
echo "$x"
x+=GHI
echo "$x"
typeset -u y=abc z
echo "$y [$z]"
z=MiXeD
echo "$z"
# A value the variable already has is converted; -l and -u replace each
# other, and together give neither.
s=Hello
typeset -u s
echo "$s"
typeset -l s
echo "$s"
typeset -lu b=aBc
echo "$b"
# Only ASCII letters change.
typeset -u n='a1-b_c.d'
echo "$n"
# Through read and for.
read x <<END
FROM READ
END
echo "$x"
for y in one Two; do
	echo "$y"
done
# With -i, and removed by +u and unset.
typeset -il i=4*4
echo "$i"
typeset +u z
z=MiXeD
echo "$z"
unset y
y=abc
echo "$y"
# local keeps the value, but not the attribute; typeset in a function makes
# a fresh variable.
typeset -u g=up
f() {
	local g
	g=down
	echo "$g"
	typeset -l x
	x=LOCAL
	echo "$x"
}
f
echo "$g $x"
# Temporary assignments before a command aren't converted.
export g
g=low env | grep '^g='
