# typeset -l, -u and -U where luish follows bash, or zsh's sh emulation
# differs (it has no path array).
# -l and -u convert each element of an array (bash; zsh doesn't) and the
# values of an associative array.
typeset -l a=(AB Cd)
a[1]+=EF
a+=(GH)
echo "${a[@]}"
typeset -A h
typeset -u h
h[k]=xy
h+=([j]=z)
h[k]+=w
echo "${h[k]} ${h[j]}"
# Values are converted when assigned (bash; zsh converts them when they are
# read), so +l keeps them as they are.
typeset -l x=ABC
typeset +l x
echo "$x"
# typeset -p, and listings by attribute.
typeset -lU a
typeset -u y=q
typeset -p a h x y
typeset -u
typeset -U
# -U on path removes repeated directories from PATH, now and on array
# assignments to path.
PATH=/a:/b:/a:/b
typeset -U path
echo "$PATH"
path+=(/c /a)
echo "$PATH"
path[0]=/c
echo "$PATH"
typeset -p path
# A string assignment to PATH doesn't use path's -U (as in zsh), but -U on
# PATH also applies to array assignments to path.
PATH=/d:/d
echo "$PATH"
typeset +U path
typeset -U PATH
path=(/e /e /f)
echo "$PATH"
