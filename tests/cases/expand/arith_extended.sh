# reference: zsh
# zsh's and bash's `**`, `++`, `--` and `,` in arithmetic.

# `**` binds tighter than `*` but not than unary minus, and is
# right-associative; it wraps as the other operators do.
echo $((2**10)) $((-2**2)) $((2**3**2)) $((2*3**2)) $((2**3*2)) $((0**0)) $((7**1)) $((2**62))
x=2; echo $((x**=3)) $x

# Increments and decrements, of variables and of array elements.
x=3; echo $((x++)) $x $((x--)) $x $((++x)) $x $((--x)) $x
x=3; echo $((x++ + ++x)) $x
x=7; echo $((!x++)) $x $((-x--)) $x $((~++x)) $x
x=5; echo $((x+++1)) $x
a=(10 20 30); echo $((a[2]++)) ${a[2]} $((--a[1])) ${a[1]} $((a[2-1]++)) ${a[1]}
unset u; echo $((u++)) $u
unset u; echo $((--u)) $u

# Blanks are allowed between the operator and the name.
x=1; echo $(( x ++ )) $x $((++ x)) $x

# `++` and `--` before or between numbers are in arith_ops.sh, as zsh
# rejects them.

# Not evaluated in the side of `&&`, `||` or `?:` that isn't taken.
x=1; echo $((0 && x++)) $x $((1 || ++x)) $x $((0 ? x-- : 0)) $x $((1 ? 0 : x--)) $x
echo $((0 && 2 ** -1))

# The comma operator, with the lowest precedence: the value of the last.
echo $((1, 2)) $((y=3, y+1)) $y
x=1; echo $((x = 5, 2)) $x
x=1; echo $((x ? 1 : 2, 3)) $(((1, 2) * 3))

# `for` loops in this style, written with `while`.
i=0; while [ $((i++)) -lt 3 ]; do printf '%s ' $i; done; echo
i=0; while [ $((++i)) -lt 3 ]; do printf '%s ' $i; done; echo

# A read-only variable can't be incremented.
readonly r=1
(: $((r++))) 2>/dev/null || echo "r: error $r"
(: $((++r))) 2>/dev/null || echo "r: error $r"
