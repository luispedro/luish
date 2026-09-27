# Suffix aliases (`alias -s`, as in zsh): a command name TEXT.SUFFIX, with
# TEXT not empty, becomes the value of the alias for SUFFIX, then the name.
# reference: zsh
unalias -a
alias -s txt='echo TXT' c='echo C:'
alias ll='echo ll'
a.txt 1 2
./b.txt
dir/c.txt x
x.y.c
V=1 d.txt
{ e.txt; }
ll f.txt
echo g.txt
.txt 2>/dev/null || echo "status $?"
a. 2>/dev/null || echo "status $?"
'q.txt' 2>/dev/null || echo "status $?"
command h.txt 2>/dev/null || echo "status $?"
# A blank at the end of the value makes the word after the name eligible
# (in zsh, not as its manual says).
alias -s txt='echo TXT '
i.txt ll
# The alias's value is not expanded as the same suffix alias again.
alias -s txt='j.txt'
k.txt 2>/dev/null || echo "status $?"
# Regular aliases come first.
alias -s txt='echo TXT'
alias l.txt='echo regular'
l.txt
