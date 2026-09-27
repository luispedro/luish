# Bad glob qualifiers are errors (status 1, as in zsh), also when the
# qualifier comes from an expansion.
# reference: zsh +o shglob -o bareglobqual
[ -n "$ZSH_NAME" ] || setopt bareglobqual
touch file
(echo *(Z)) 2>/dev/null; echo $?
(echo *(L)) 2>/dev/null; echo $?
(echo *(u:nosuchuserluish:)) 2>/dev/null; echo $?
y='file(Z)'
(echo $y) 2>/dev/null; echo $?
echo "$y" '*(Z)' \*\(Z\)
echo *(Z)
echo not reached
