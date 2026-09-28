# reference: zsh
# typeset -U (zsh): an array keeps only the first of equal elements.
typeset -U a=(x y x z y)
echo "${a[@]}"
a+=(x w)
echo "${a[@]}"
a[3]=x
echo "${a[@]}"
a[1]+=z
a=("${a[@]}" yz x)
echo "${a[@]}"
b=(1 1 '' 2 '' 1)
typeset -U b
echo "${#b[@]} ${b[@]}"
# A string isn't changed, except PATH, whose repeated directories are
# removed.
typeset -U s=a:b:a
echo "$s"
typeset -U PATH
PATH=/c:/c:/d:/c
echo "$PATH"
typeset +U a
a+=(x)
echo "${a[@]}"
