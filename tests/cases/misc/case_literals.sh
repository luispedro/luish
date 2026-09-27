# case patterns that are plain literals, and ones that only look like it.
for w in a b '*' '\' '[' 'a b' '' x\\y abc; do
	case $w in
	a) echo "$w: a" ;;
	\*) echo "$w: star" ;;
	'\') echo "$w: backslash" ;;
	\[) echo "$w: bracket" ;;
	'a b') echo "$w: space" ;;
	'') echo "$w: empty" ;;
	x\\y) echo "$w: escaped" ;;
	a*) echo "$w: a-star" ;;
	*) echo "$w: other" ;;
	esac
done
case b in [ab]) echo set ;; esac
case '[ab]' in [ab]) echo wrong ;; '[ab]') echo quoted ;; esac
