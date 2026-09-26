# `__luish_internal savestate` prints commands that restore the shell's
# state: a new shell that runs them has the same state (and prints the same
# commands).
mkdir d
cd d
umask 027
x="it's" y='a
b'
readonly r=1
export e
alias ll='ls -l' q='"'
f() { cat <<X; echo "${1:-none}" | tr a-z A-Z; }
hi $x
X
g() { case $1 in a|b) echo ab;; *) echo "other: $(echo "$1")";; esac; }
# Defined before the alias, so its body calls the function. The alias is
# also defined when the state is read back, which mustn't expand it.
k() { greet; }
greet() { echo "greet $*"; }
alias greet='greet loudly'
trap 'echo bye' EXIT
dirs /x "/it's here"
set -u -o noclobber
__luish_internal savestate > ../state
$SH -c '. ../state; __luish_internal savestate > ../state2'
cmp ../state ../state2 && echo same
$SH -c 'alias greet="greet loudly"; . ../state
f; f x; g a; g "q r"; k; greet; printf "[%s]\n" "$x" "$r" "$y" "${e-unset}"
alias; umask; case $PWD in */d) echo in d;; esac; dirs -p | sed 1d; echo $-; env | grep "^e="'
echo status $?
sed -n '/^k()/,/^}/p' ../state
__luish_internal 2>&1; echo "status $?"
__luish_internal nosuch 2>&1; echo "status $?"
__luish_internal savestate extra 2>&1; echo "status $?"
