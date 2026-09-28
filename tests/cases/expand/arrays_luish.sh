# Where luish's arrays differ from zsh's sh emulation.
# A string is an array of one element, as in bash (zsh indexes its
# characters).
s=abc
echo "${s[0]}" "[${s[1]-unset}]" ${#s[@]} "${s[@]}"
s[1]=q
printf '<%s>' "${s[@]}"
echo
# An index before the start is an error, as in bash (zsh prepends).
a=(x y z)
(a[-4]=w; echo never)
echo "status $?"
(: $((a[-4] = 1)); echo never)
echo "status $?"
# `unset 'a[@]'` unsets the array, as in bash.
unset 'a[@]'
echo "${a-unset}"
# Assignments to an element before a command are temporary, as the others.
a=(x y)
a[1]=Q true
echo "${a[@]}"
f() { echo "in f: ${a[@]}"; }
a[0]=R f
echo "${a[@]}"
# `local a` keeps the value, as `local x` does in dash (zsh unsets it).
g() { local a; echo "${a[@]}"; a[1]=Q; echo "${a[@]}"; }
g
echo "${a[@]}"
# `set` and `readonly -p` quote each element.
b=(1 "2 3")
set | grep '^b='
readonly b
readonly -p | grep ' b='
# The state can be saved and restored.
c=(x "it's" '')
__luish_internal savestate > state
$SH -c '. ./state; printf "<%s>" "${c[@]}" "${b[@]}"; echo; b=1'
echo "status $?"
