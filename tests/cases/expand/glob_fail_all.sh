# setopt glob.fail_all (zsh's CSH_NULL_GLOB): patterns that match nothing
# are removed, and it is an error (exiting the shell, here the subshells)
# only if none of the patterns of the command, `for` or array matches.
# reference: zsh
mkdir t; cd t
touch a b
setopt CSH_NULL_GLOB
echo 1 x* a* y*
(echo 2 x* plain; echo not reached); echo "2 status $?"
(echo 3 x* y*; echo not reached); echo "3 status $?"
for i in x* b*; do echo 4 $i; done
(for i in x* y*; do echo $i; done); echo "5 status $?"
arr=(x* a*); echo 6 "${arr[@]}"
(arr=(x* y*); echo not reached); echo "7 status $?"
echo 8 $(echo x* y*) after
echo 9 "x*" plain
(setopt NOMATCH; echo 10 x* a*)
echo 11 x* y*
echo not reached
