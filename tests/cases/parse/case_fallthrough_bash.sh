# bash's `;;&`, which zsh spells `;|`: the next arms' patterns are tried.
for x in ab b x; do
	case $x in
	a*) echo "starts with a" ;;&
	*b) echo "ends with b" ;;&
	b) echo "is b" ;;
	*) echo "anything: $x" ;;
	esac
done
case a in a) echo last ;;& esac
f() { case $1 in a)echo fa;;&*)echo any;;esac; }
f a
