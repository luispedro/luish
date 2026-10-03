# `<<< word`, a here-string (bash, zsh): the word is expanded without
# splitting or globbing, and given on stdin with a newline added.
# reference: zsh
touch f1 f2
x="a  b"
cat <<< $x
cat <<<*
cat <<< ~ | sed "s|^$HOME|HOME|"
cat <<<""
cat <<< 'single $x'
cat <<< "double $x"
cat <<<$(echo sub)x
read a b <<< "1 2"; echo "$b-$a"
set -- p q r
cat <<< "$@"
cat <<< "$*"
# A file descriptor, and the order of redirections.
cat 3<<< three <&3
cat <<< first <<< second
cat <<< both 2>/dev/null
# Compound commands and functions; a function's body keeps it.
{ cat; cat; } <<< grp
while read l; do echo "got $l"; done <<< loop
f() { cat <<< "in $1"; }
f fn
eval "$(typeset -f f | sed 's/^f/g/')"
g again
# `<<<` doesn't start a here-document.
cat <<< x; echo after
