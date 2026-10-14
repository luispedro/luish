# setopt glob.null (zsh's NULL_GLOB, bash's nullglob): a pattern that
# matches nothing is removed, from commands, `for` and arrays. Quoted
# patterns, assignments, `case` and redirections are left alone.
# reference: zsh
mkdir t; cd t
touch a b .h
setopt NULL_GLOB
echo 1 x* a
set -- x* y*; echo 2 $#
for i in x* a*; do echo 3 $i; done
arr=(x* b); echo 4 ${#arr[@]} "${arr[@]}"
echo 5 "x*" 'y*' x\* x[
v='x*'; echo 6 $v "$v"
echo 7 $(echo inner x*) end
f=x*; echo "8 $f"
case x in x*) echo 9;; esac
echo 10 */ d*/
# zsh's precedence: glob.null wins over glob.fail and glob.fail_all.
(setopt NOMATCH CSH_NULL_GLOB; echo 11 x* y*)
unsetopt NULL_GLOB; echo 12 x*
echo 13 > x*; echo *
