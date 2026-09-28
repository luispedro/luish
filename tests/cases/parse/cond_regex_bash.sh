# After `=~` in `[[ ... ]]`, `(` and `|` are part of the word, and so is
# anything inside parentheses, as in bash (zsh needs these quoted).
[[ abc =~ ^(a|x)b ]] && echo "1 $MATCH"
[[ ab =~ a|x ]] && echo "2 $MATCH"
[[ 'a b;c' =~ ^(a b;c)$ ]] && echo "3 $MATCH"
[[ 'x<y' =~ (x<y) ]] && echo "4 $MATCH"
[[ ab =~ (a)(b) && -n x ]] && echo "5 $MATCH"
[[ ab =~ ((a)|c)b || x ]] && echo "6 $MATCH"
v=b; [[ ab =~ (a)($v) ]] && echo "7 $MATCH"
[[ ab =~ (
a) ]] || echo "8 line $LINENO"
f() { [[ $1 =~ ^(a|b c)+$ ]]; }
f 'ab ca' && echo "9 function"
