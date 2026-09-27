# Where luish's `alias` differs from both dash and zsh. As in dash, values
# are always quoted, and errors are dash's (status 2 for a bad option).
# Unlike dash, arguments that start with - or + are options (as in zsh),
# so a name that starts with one needs `--`. Unlike zsh, a here-document
# delimiter isn't expanded as a global alias, `command -v` shows a suffix
# alias as a command, and `alias -L` quotes names that need it.
err() { sed 's/^.*: \([un]*alias: \)/\1/'; }
alias x=y
alias; alias -L
alias -- -x='echo dash x' '+y=echo plus y' 'a b=c'
alias -L
alias -z 2>&1 | err; alias -z 2>/dev/null; echo "status $?"
alias +x=1 2>&1 | err
alias -gs q=1 2>&1 | err; alias -gs q=1 2>/dev/null; echo "status $?"
alias -rg q=1 2>/dev/null; echo "status $?"
alias -s txt='cat -n'
alias -s x 2>&1 | err; alias -s x 2>/dev/null; echo "status $?"
command -v a.txt
unalias -s x 2>&1 | err
unalias -a x; alias; echo "status $?"
unalias -z 2>/dev/null; echo "status $?"
alias -g EOF='echo expanded'
cat <<EOF
here-document
EOF
