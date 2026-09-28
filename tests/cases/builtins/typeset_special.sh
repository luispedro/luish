# typeset -p prints the specials that are still special: path, dirstack and
# pipestatus as arrays, the others as strings, with their attributes.
PATH=/usr/bin:/bin
typeset -p path
path+=(/opt/bin)
typeset -p path
saved=$(typeset -p path)
path=(/x)
eval "$saved"
echo "$PATH"
dirstack=(/a "/b c")
typeset -p dirstack
false | true
typeset -p pipestatus PIPESTATUS
typeset -p EUID | sed 's/[0-9][0-9]*/N/'
path=x
typeset -p path
unset pipestatus
typeset -p pipestatus
echo $?
readonly dirstack
typeset -rp
