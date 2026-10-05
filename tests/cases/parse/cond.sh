# `[[ ... ]]`, as in zsh and bash.
# reference: zsh
# Patterns on the right of `=`, `==` and `!=`: quoted parts, and quoted
# expansions, match literally.
p='a*'
[[ abc = a* ]] && echo 1; [[ abc = "a*" ]] || echo 2
[[ abc == $p ]] && echo 3; [[ abc = "$p" ]] || echo 4
[[ a* = "$p" ]] && echo 5; [[ abc != a?c ]] || echo 6
[[ b = [ab] ]] && echo 7; [[ 'x y' = x\ * ]] && echo 8
# No field splitting or globbing, and no problem with empty values.
touch f1 f2
x='a  b' empty=
[[ $x = 'a  b' ]] && echo 9
[[ f* = f1 ]] || echo 10; [[ -e f* ]] || echo 11
[[ -z $empty && ! -n $empty ]] && echo 12
[[ $empty ]] || echo 13; [[ $x ]] && echo 14
[[ ~ = "$HOME" ]] && echo 15
# A word that looks like an operator is just a word where one is expected.
[[ -n -f ]] && echo 16; [[ -z -n ]] || echo 17
# As in zsh, `-n = x` compares strings.
[[ -f = -f && -n != x ]] && echo 17b
[[ '-f' ]] && echo 18; [[ "!" = ! ]] && echo 19
# File tests.
mkdir d; ln -s f1 link
[[ -f f1 && -d d && -e d && -a f1 ]]; echo "$?"
[[ -h link && -L link && ! -h f1 ]]; echo "$?"
[[ -s f1 ]]; echo "$?"; echo x > f1; [[ -s f1 && -r f1 && -w f1 ]]; echo "$?"
[[ -x d && ! -x f1 ]]; echo "$?"
[[ -e nonexistent || -f d ]]; echo "$?"
[[ f1 -ef link && ! f1 -ef f2 ]]; echo "$?"
[[ -t 5 ]]; echo "$?"
# Variables and options.
set -- a b
v=1
[[ -v v && -v 2 && ! -v 3 && ! -v nosuchvar && -v HOME ]]; echo "$?"
[[ -o noglob ]]; echo "$?"; set -f; [[ -o noglob && -o noglob ]]; echo "$?"; set +f
[[ -o nosuchoption ]]; echo "$?"
# Strings compare as bytes.
[[ a < b && b > a && ! b < a && 10 < 9 && A < a ]]; echo "$?"
[[ a<b ]]; echo "$?"
[ -e b ] || echo "no file named b"
# Numbers are arithmetic expressions.
n=3
[[ n -eq 3 && 1+2 -eq n && n*2 -gt 5 && " 7 " -ge 7 ]]; echo "$?"
[[ $n -lt 3 || $n -le 2 || $n -ne 3 ]]; echo "$?"
[[ empty -eq 0 && 0x10 -eq 16 ]]; echo "$?"
# `!` binds most tightly, then `&&`, then `||`; parentheses group.
[[ ! a = b && c ]]; echo "$?"
[[ a = b && c || d ]]; echo "$?"
[[ a = b && ( c || d ) ]]; echo "$?"
[[ ! ( a = a || x ) ]]; echo "$?"
[[ ! ! a ]]; echo "$?"
[[ (a) && ( ( b ) ) ]]; echo "$?"
# Words are expanded only if they are needed.
[[ a = b && -n $(touch ran1) ]]; [[ a = a || -n $(touch ran2) ]]
[ -e ran1 ] || [ -e ran2 ] || echo "not run"
[[ -n $(echo a) && $(printf 'x\ny') = 'x
y' ]] && echo "substitutions"
# Newlines between the words.
[[
    a = a &&
    b
]] && echo "newlines"
# A command: redirections, pipelines, `&&`, `!`, and in conditions.
[[ a ]] >out && echo "status $?"
[[ a = b ]] | cat; echo "pipeline $?"
! [[ a = b ]] && echo "negated"
if [[ $1 = a ]]; then echo "if"; fi
i=0; while [[ $i -lt 3 ]]; do i=$((i+1)); done; echo "while $i"
case x in x) [[ x ]] && echo "in case";; esac
f() { [[ $1 = a* ]]; }; f abc && echo "function"
# Only a command name is reserved.
echo [[ a ]] ]]
for w in [[ ]]; do printf '%s ' "$w"; done; echo
# Regular expressions: quotes are only removed, and a match sets MATCH,
# MBEGIN and MEND.
re='^a.c$'
[[ abc =~ $re ]]; echo "$?"
[[ a.c =~ "a.c" && abc =~ "a.c" ]]; echo "$?"
[[ abc =~ 'b|x' ]]; echo "$?"
[[ abc =~ ^b ]]; echo "$?"
[[ xabcx =~ 'b+c' ]] && echo "$MATCH $MBEGIN $MEND"
MATCH=old; [[ abc =~ x ]]; echo "$? $MATCH"
[[ abc =~ '' ]] && echo "<$MATCH> $MBEGIN $MEND"
[[ a =~ '(' ]] 2>/dev/null; echo "$?"
[[ ! a =~ '(' ]] 2>/dev/null; echo "$?"
# `set -e` applies to it as to a simple command.
set -e
[[ a = b ]] || echo "exempt"
! [[ a = a ]]
[[ a = a ]] && [[ a = b ]] && echo "not here"
echo "still running"
[[ a = b ]]
echo "not reached"
