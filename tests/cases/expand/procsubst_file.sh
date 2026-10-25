# reference: zsh
# =(...) is a temporary file with the list's output, removed when the command
# is over.
TMPDIR=$HOME/tmp
mkdir "$TMPDIR"
cat =(echo hi)
cat =(echo a) =(echo b)
diff =(echo a) =(echo b)
echo status=$?
# A regular file that only its owner can read and write, in $TMPDIR (zsh uses
# $TMPPREFIX).
f() {
    [ -f "$1" ] && echo regular
    ls -l "$1" | cut -c1-10
    case $1 in "$TMPDIR"/luish-*|/tmp/zsh*) echo temporary;; esac
    p=$1
}
f =(echo x)
[ -e "$p" ] || echo removed
# Not split, and a redirection target.
set -- =(echo 'a b')
echo $#
cat < =(echo redirected)
cat <=(echo after the operator)
# Nested, and with a pipe substitution.
cat =(cat =(echo nested))
cat =(echo file) <(echo pipe)
# The list's status is not the command's, and set -e ignores it.
true =(exit 3)
echo status=$?
(set -e; cat =(false); echo after false)
# In a background job, a subshell, and the last command of a shell.
cat =(echo background) &
wait
(cat =(echo subshell))
$SH -c 'cat =(echo last)'
# exec keeps the file open after removing it.
exec 3< =(echo from fd 3)
cat <&3
exec 3<&-
# Only at the start of a word, and not quoted or in ${...} or $((...)).
echo ${x-=(y)} "${x:-=(y)}" '=(y)' "=(y)"
a=1
echo $((a =(2))) $a
ls "$TMPDIR" | wc -l
