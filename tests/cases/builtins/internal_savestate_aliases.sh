# Global and suffix aliases in `__luish_internal savestate`: the commands
# from the aliases on are grouped in braces, so that a global alias doesn't
# change the words after it, and words in function bodies that would be
# expanded as aliases are quoted.
f() { echo G set >G; }
g() { a.txt; }
alias ll='echo ll' -x='echo dash x'
alias -g G='| tr a-z A-Z' set=SET
alias -s txt='echo TXT'
__luish_internal savestate > state
tail() { sed -n '/^f()/,$p' "$1" | grep -v '^set [-+]o\|^setopt\|^unsetopt'; }
tail state
$SH -c '. ./state; __luish_internal savestate > state2'
tail state > a; tail state2 > b; cmp a b && echo same
# Read into a shell that has some of the aliases already: they aren't
# expanded in the function bodies. (Any alias there may change the other
# commands, as for regular aliases.)
$SH -c 'alias -g G=nope; alias -s txt=nope; . ./state; f; cat G; alias -L; alias -Ls; type g'
