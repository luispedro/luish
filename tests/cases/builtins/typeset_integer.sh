# reference: zsh
# typeset -i: assignments to an integer variable are evaluated as arithmetic
# expressions.
typeset -i x
echo "[$x]"
x=1+2
echo "$x"
x='3 * 4'
echo "$x"
x+=5
echo "$x"
x+=1+1
echo "$x"
x=
echo "$x"
y=7
x=y*2
echo "$x"
x=unset_name
echo "$x"
typeset -i a=2*3 b c=a+1
echo "$a $b $c"
# A value the variable already has is evaluated.
s=2+3
typeset -i s
echo "$s"
# Through read, for and arithmetic.
read x <<END
2*8
END
echo "$x"
for x in 1+1 3*3; do
	echo "$x"
done
: $((x = 40 + 2))
echo "$x"
# typeset +i, unset and local remove the attribute; typeset in a function
# makes a fresh variable.
f() {
	local x=1+1
	echo "$x"
	typeset c=1+1
	echo "$c"
	local -i n=3*3
	echo "$n"
}
f
x=1+2
echo "$x"
typeset +i x
x=1+2
echo "$x"
unset a
a=1+1
echo "$a"
# Temporary assignments before a command aren't evaluated.
typeset -i t=4
export t
t=2+2 env | grep '^t='
g() { echo "$t"; }
t=1+1 g
# An error in the expression exits the shell.
typeset -i e
(e=1+; echo not reached) 2>/dev/null || echo failed
