# `function NAME`, as in zsh (and bash): with or without `()`, a name that
# isn't a variable name, several names, and a body on the next line.
# reference: zsh
function f { echo "f $# $1"; }
f a b
function g() { echo g; }; g
function h ()
{
    echo h
}
h
function git-up a.b c:d { echo "multi $1"; }
git-up 1; a.b 2; c:d 3
function if { echo "named if"; }
\if
# The body keeps its redirections, and the definition's status is 0.
function r { echo to-file; } >out
r; cat out
false; function s { :; }; echo "status $?"
# The name isn't alias-expanded (it is after `f()`).
alias al=other
function al { echo "not expanded"; }
\al
unalias al
# Only a command name is reserved.
echo function
for function in x; do echo "for $function"; done
case function in function) echo matched;; esac
x=function; echo "$x"
# Redefining, unsetting, and defining inside a function.
function f { echo redefined; }; f
unset -f git-up; git-up 2>/dev/null || echo "unset $?"
function outer { function inner { echo inner; }; }
outer; inner
f() { echo posix; }; f
