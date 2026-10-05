# reference: zsh
# `;&` runs the next arm's body too, without trying its patterns; `;|`
# goes on trying the next arms' patterns.
for x in a b c d; do
	case $x in
	a) echo "a" ;&
	b) echo "a or b" ;&
	c) echo "a, b or c" ;;
	*) echo "other: $x" ;;
	esac
done

for x in ab b x; do
	case $x in
	a*) echo "starts with a" ;|
	*b) echo "ends with b" ;|
	b) echo "is b" ;;
	*) echo "anything: $x" ;;
	esac
done

# The status is the last body's; without a match after `;|`, it stays.
case a in a) false ;| b) true ;; esac
echo "status $?"
case a in a) true ;& b) false ;; esac
echo "status $?"
case a in a) false ;& esac
echo "status $?"
case a in a) (exit 3) ;& b) ;; esac
echo "status $?"

# The last arm may end with either, and the fall-through ends there.
case a in a) echo last ;& esac
case a in a) echo last ;| esac

# Without blanks, and in a function, whose definition shows them.
f() { case $1 in a)echo fa;&b)echo fb;|*)echo any;;esac; }
f a
f b
typeset -f f | grep -c ';&'

# break and return inside a body that falls through.
for x in a; do
	case $x in a) echo one; break ;& b) echo not reached ;; esac
done
g() { case a in a) return 4 ;& b) echo not reached ;; esac; }
g
echo "g: $?"
