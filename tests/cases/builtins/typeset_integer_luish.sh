# typeset -i with arrays (as in bash; zsh has no integer arrays), typeset -p,
# and values printed in decimal (zsh uses the base of the first number).
typeset -ia a=(1+1 2*3)
echo "${a[@]}"
a[3]=2+2
a+=(3*3)
a[0]+=1
echo "${a[@]}"
typeset -iA h
h[x]=1+1
h[x]+=2
echo "${h[x]}"
h=(k 1+2)
echo "${h[k]}"
typeset -i o=010
echo "$o"
o=0x10
echo "$o"
typeset -p a h o
typeset -i u
typeset -p u
export o
typeset -p o
typeset -ir r=5*5
typeset -p r
typeset -i
typeset +i o
typeset -p o
