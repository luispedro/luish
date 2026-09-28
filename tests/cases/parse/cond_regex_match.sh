# `=~` in `[[ ... ]]` sets the arrays `match`, `mbegin` and `mend` for the
# groups of the regular expression, as in zsh (offsets from 0, as in its sh
# emulation).
# reference: zsh
re='b(c)(x)?(d)'
[[ abcd =~ $re ]] && echo "1 $MATCH $MBEGIN $MEND"
echo "2 ${#match[@]} [${match[0]}] [${match[1]}] [${match[2]}]"
echo "3 ${mbegin[*]} / ${mend[*]}"
# An empty group.
re='(x?)b'
[[ ab =~ $re ]] && echo "4 [${match[*]}] ${mbegin[*]} ${mend[*]}"
# In arithmetic.
re='a(b+)c'
[[ xabbbc =~ $re ]] && echo "5 $((mend[0] - mbegin[0] + 1))"
# A failed match, or one without groups, leaves them as they were.
re='(q)'
[[ abc =~ $re ]]; echo "6 $? ${match[*]} ${mbegin[*]} $MATCH"
[[ abc =~ c ]]; echo "7 ${match[*]} ${mbegin[*]} $MATCH"
# They are ordinary variables: a string becomes an array, and `local`
# keeps the match inside a function.
match=x
re='(b)'
[[ ab =~ $re ]]; echo "8 ${#match[@]} ${match[*]}"
f() {
	local match mbegin mend
	re='(a)(b)'
	[[ ab =~ $re ]] && echo "9 ${match[*]} ${mbegin[*]}"
}
f; echo "10 ${match[*]} ${mbegin[*]}"
# A read-only one is an error.
(readonly mend; [[ ab =~ $re ]]; echo not reached) 2>/dev/null
echo "11 ${match[*]}"
