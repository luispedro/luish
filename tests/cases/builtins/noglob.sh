# reference: zsh
# `noglob`, as in zsh: the words of the command aren't globbed, but those
# of a `$(...)` in them are, and the other expansions are made.
touch a b
noglob echo * [ab] a?
noglob echo *.none "$(echo *)" $(echo *)
x='* ?'
noglob echo $x
noglob printf '<%s>\n' ~/* | sed "s|$HOME|HOME|"
noglob noglob echo *
echo * after
alias m='noglob echo'
m *
m * | cat
f() { echo "f $#: $*"; }
noglob f * b*
v=1 noglob f * "$v"
noglob command echo *
noglob builtin echo *
noglob >out echo *; cat out; rm out
noglob; echo "alone $?"
noglob false; echo "false $?"
(noglob exec echo * exec)
builtin noglob echo b* | cat
for i in 1; do noglob echo * in loop; done
if noglob test '*' = *; then echo condition; fi
type noglob
noglob true *; echo "status $?"
builtin noglob builtin noglob echo b* twice
n=noglob; $n echo b* from a variable; "noglob" echo b* quoted
set -- noglob echo; "$@" b* split
empty=; $empty noglob echo b* after an empty word
builtin echo b* builtin alone
