# `=~` in `[[ ... ]]` sets `BASH_REMATCH` to the matched text and those of
# the groups, as in bash (and zsh's `BASH_REMATCH` option).
# reference: zsh -o bashrematch
re='b(c)(x)?(d)'
[[ abcd =~ $re ]]
echo "1 ${#BASH_REMATCH[@]} [${BASH_REMATCH[0]}] [${BASH_REMATCH[1]}] [${BASH_REMATCH[2]}] [${BASH_REMATCH[3]}]"
[[ abc =~ c ]]; echo "2 ${#BASH_REMATCH[@]} $BASH_REMATCH"
# A failed match leaves it as it was, as in zsh (bash empties it).
[[ abc =~ x ]]; echo "3 ${#BASH_REMATCH[@]} $BASH_REMATCH"
re='^([^=]*)=(.*)$'
for kv in a=1 b=2=3; do
	[[ $kv =~ $re ]] && echo "4 ${BASH_REMATCH[1]} -> ${BASH_REMATCH[2]}"
done
