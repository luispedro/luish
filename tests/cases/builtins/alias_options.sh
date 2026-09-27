# zsh's options to `alias` and `unalias`: -g and -s define global and suffix
# aliases; for printing, -g, -r and -s select global, regular or suffix
# aliases, -m takes patterns, -L prints commands, and + prints names only.
# (The values have blanks, since zsh only quotes them when needed.)
# reference: zsh
unalias -a
alias ll='ls -l' xx='echo x'
alias -g G='| grep' '...=cd ../..'
alias -s txt='cat -n' pdf='open it'
echo '--- all but suffix aliases'; alias
echo '--- -g'; alias -g
echo '--- -r'; alias -r
echo '--- -s'; alias -s
echo '--- -L'; alias -L
echo '--- -Lg, -Ls'; alias -Lg; alias -Ls
echo '--- +, +g, +s, -g +'; alias +; alias +g; alias +s; alias -g +
echo '--- names'; alias ll 'G'; echo "status $?"
echo '--- -g, -r with names'; alias -g ll 'G'; alias -r ll 'G'; echo "status $?"
echo '--- -s with names'; alias -s txt; echo "status $?"
echo '--- -L and + with names'; alias -L ll 'G'; alias -Ls txt; alias + ll 'G'; alias +g ll 'G'
echo '--- -m'; alias -m 'l*'; alias -gm '*'; alias -m + '*'; alias -Lm '*l*'; alias -sm 't*'
alias -m 'zz*'; echo "status $?"
alias -m 'zz=1'; echo "status $?"; alias +
echo '--- -r redefines a global alias as regular'; alias -r G='g2 x'; alias -Lm 'G'
alias 'G=| grep'; alias -Lm 'G'
echo '--- type'; alias -g G='| grep'; type ll G a.txt; command -v G; command -V G; command -V a.txt
echo '--- unalias -s'; unalias -s txt; alias -s; echo "status $?"
echo '--- unalias -m'; unalias -m 'x*' '...'; alias +; echo "status $?"
unalias -m 'nomatch'; echo "status $?"
unalias -sm 'p?f'; alias -s; echo "status $?"
echo '--- unalias -a keeps suffix aliases'
alias -s a='x y' b='y z'; unalias -a; alias; alias -s
echo '--- unalias -as'; unalias -as; alias -s; echo "status $?"
