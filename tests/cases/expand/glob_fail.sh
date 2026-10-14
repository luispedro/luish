# setopt glob.fail (zsh's NOMATCH, bash's failglob): a pattern that matches
# nothing is an error, which exits the shell (here, the subshells).
# Quoted patterns, assignments, `case` and redirections are left alone.
# reference: zsh
mkdir t; cd t
touch a
setopt NOMATCH
(echo 1 x* a; echo not reached); echo "1 status $?"
echo 2 a* "x*" x\* x[
(for i in x*; do echo $i; done); echo "3 status $?"
(arr=(a x*); echo not reached); echo "4 status $?"
echo 5 $(echo x*; echo not reached) end
(f() { echo x*; }; f; echo not reached); echo "6 status $?"
v=x*; echo "7 $v"
case x in x*) echo 8;; esac
(setopt NULL_GLOB; echo 9 x* a)
(unsetopt NOMATCH; echo 10 x*)
(echo 11 a | echo x*); echo "11 status $?"
echo 12 x*
echo not reached
