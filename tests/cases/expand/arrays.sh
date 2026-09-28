# reference: zsh
# Arrays, as in zsh's sh emulation (KSH_ARRAYS) and bash: indices from 0,
# `$a` is `${a[0]}`.
a=(x "y z" w)
echo $a "${a}" ${a[1]} "${a[2]}" ${a[-1]} ${a[-3]} "[${a[3]-unset}]" "[${a[-4]-unset}]"
printf '<%s>' "${a[@]}" ${a[@]} "${a[*]}" ${a[*]}
echo
echo ${#a} ${#a[1]} ${#a[@]} ${#a[*]}
i=1
echo ${a[i]} ${a[i+1]} ${a[$i]} ${a[ 2 ]} ${a[$((i))]}
# The elements are expanded as command words: split, globbed.
touch f1 f2
v="1 2"
b=( $v "$v" f*
  ~ $(echo 3 4) # comment
  '' )
printf '<%s>' "${b[@]}"
echo " ${#b[@]}"
# Empty elements, and IFS.
c=(x "" z)
printf '<%s>' ${c[@]} "${c[@]}" ${c[*]} "${c[*]}"
echo
IFS=:
printf '<%s>' "${c[*]}"
s=${c[*]} t=${c[@]}
echo " $s $t"
IFS=' '
# Assigning.
a=(x y z)
a=q
echo "${a[@]}" ${#a[@]}
a[0]=r a[4]=v
printf '<%s>' "${a[@]}"
echo
a[-1]=last
echo "${a[@]}"
a=(x y)
a+=(z w)
a+=q
a[1]+=Q
a[6]+=R
printf '<%s>' "${a[@]}"
echo
s=abc
s+=(z)
printf '<%s>' "${s[@]}"
echo
x=a
x+=b
unset y
y+=c
echo $x $y
e=()
echo ${#e[@]} "[${e[*]}]" "[${e[@]:-empty}]"
for x in "${e[@]}"; do echo never; done
# `$e` is `${e[0]}`, which an empty array doesn't have.
echo "[${e+set}] [${e-unset}] [${e:-e}] [${#e}] [${e[@]+set}]"
(set -u; echo "$e"; echo never)
# Operators apply to each element; `"${a[*]/...}"` to the joined string.
a=(ab cb)
echo ${a[@]/b/X} ${a[@]#?} ${a[@]%b} "${a[*]/b/X}" ${a[*]#?} "${a[@]//b}"
a=(x y z)
echo ${a[@]:1} ${a[@]: -1} ${a[*]:0:2} ${a[@]:1:1} ${a:1}
echo ${a[@]:-def} ${u[@]:-def} ${a[@]+set} ${u[@]+set} ${a[1]:+alt} ${a[7]-def}
echo ${a[1]:=Q} ${u[1]=new}
echo "${u[@]}" ${#u[@]}
for i in "${a[@]}"; do echo "- $i"; done
# unset: an element is emptied.
a=(x y z)
unset 'a[1]'
echo ${#a[@]} "${a[@]}"
unset 'a[-1]' 'a[7]'
printf '<%s>' "${a[@]}"
echo
unset a
echo "${a-unset}" ${#a[@]}
# Arithmetic.
i=1
a=(1 2 3)
echo $((a[1] + 1)) $((a[i] * a[i+1])) $((a[-1])) $((a[5])) $((a))
: $((a[1] = 10)) $((a[2] += 5)) $((a[4] = 7))
echo "${a[@]}"
i=0
: $((a[i=1] *= 3))
echo "${a[@]}" $i
echo $((1 ? a[1] : a[0])) $((0 && (a[1] = 9)))
let 'a[0] = 5' 'a[1] += 1'
echo "${a[@]}"
# Declaration commands.
f() {
  local a=(1 "2 3") b=x c=() d=("")
  printf '<%s>' "${a[@]}" "$b" "${#c[@]}" "${#d[@]}"
  echo
}
a=(q)
f
echo "${a[@]}"
export g=(x "y z")
echo "${g[1]}"
env | grep '^g='
readonly h=(1 2)
echo "${h[@]}"
