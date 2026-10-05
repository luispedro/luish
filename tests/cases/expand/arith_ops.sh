# Operators that share a prefix, compound assignments, and invalid tokens.
x=3; echo $((x<<=2)) $x $((x>>=1)) $x $((x%=4)) $x $((x^=7)) $x $((x&=5)) $x $((x|=8)) $x $((x/=3)) $x
echo $((1<2<3)) $((3>2>1)) $((1==1!=0)) $((!!5)) $((~~5)) $((- -3)) $((+-3)) $((1 - -1)) $((5&&3||0))
echo $((0 && (y=1))) ${y-unset} $((1 || (y=1))) ${y-unset} $((0 ? y=1 : 2)) ${y-unset}
echo $((0x7fffffffffffffff)) $((0777)) $((0xAbC)) $((  12  ))
v=' 0x10 '; w=-010; echo $((v+1)) $((w))
for e in '08' '0x' '1 +=2' '1 = 2' '(1' '1 ? 2' '5++' '1 >>>= 2' '@'; do
	(eval "echo \$(($e))") 2>/dev/null || echo "error: $e"
done
# Before anything but a name, `++` and `--` are two unary operators (as in
# bash; zsh rejects them), and so are binary and unary ones between numbers.
echo $((--5)) $((++5)) $((5--1)) $((5++1)) $((-- 5))
v=12abc; (echo $((v))) 2>/dev/null || echo bad
