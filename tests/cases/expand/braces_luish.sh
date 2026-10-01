# setopt expand.braces where bash and zsh differ: luish follows bash, but
# for the ends of a sequence, which can be expansions (expanded once), and
# for sequences of any characters.
setopt expand.braces
# The sign of the step is ignored; a step of 0 is 1; characters can have a
# step; a sign can be `+`.
echo 1 {1..10..-3} {10..1..-3} {1..3..0} {a..e..2} {+1..3}
# An integer and a character aren't a sequence.
echo 2 {1..a} {a..9} {1..3..x}
# Empty words are removed, as other unquoted empty words are.
printf '<%s>' {a,} {,} x{,}y ''{a,}
echo
# Tilde expansion comes after.
HOME=/h
echo 3 ~{,/tmp} {~,~/x} a{~,b}
# A stray `}` is text (zsh: a parse error).
echo 4 {a,b}} a,b}
# An expansion in a sequence is expanded once, also when it doesn't give
# one (zsh as well); its value is then quoted.
i=0
echo 5 {1..$((i += 2))} {a..$((i += 1))} $i
s='x*'
touch '{a..xz'
echo 6 {a..$s}
# The braces of the assignments of declaration commands aren't expanded
# (as in zsh; bash's `declare x={a,b}` is `declare x=a x=b`).
f() { local v={a,b} w; w=({a,b}); echo 7 $v ${w[@]}; }
f
# A redirection to more than one file is an error, as in bash (zsh writes
# to both).
echo hi > o{1,2}
echo 8 $?
echo hi > o{1}
echo 9 $(cat o{1})
# Characters beyond ASCII.
echo 10 {α..γ}
# The command's name is expanded too, as in bash (zsh: not found).
{echo,11}
unsetopt expand.braces
echo 12 {a,b} {1..2}
