# reference: zsh
# Associative arrays (`typeset -A`, as in zsh and bash). Their order isn't
# the same in any two shells, so the lists are sorted.
typeset -A h
echo "${#h[@]} [${h[@]}]"
h[a]=1
k='b c'
h[$k]=2
echo "${h[a]} ${h[$k]} ${h[b c]} ${#h[@]}"
# A key is a string, not an arithmetic expression.
h[1+1]=x
echo "${h[1+1]} [${h[2]-unset}]"
# Operators apply to an element.
echo "${h[a]:-d} ${h[q]:-d} ${#h[$k]} ${h[$k]/2/two} [${h[q]+set}]"
echo "${h[q]=w} ${h[q]}"
# Assigning a list: pairs of keys and values, or `[key]=value`, whose value
# is expanded as in an assignment.
h=(k1 v1 k2 v2)
echo "${#h[@]} ${h[k1]} ${h[k2]}"
x='1 2'
h=([a]=$x ["x y"]=f* [$k]=3 ['']=e)
echo "${#h[@]} ${h[a]} ${h[x y]} ${h[b c]} ${h['']}"
h+=([z]=4 [a]=5)
h+=(y 6)
h[a]+=0
echo "${#h[@]} ${h[a]} ${h[z]} ${h[y]}"
# `${h[@]}` and `${h[*]}` are the values.
for v in "${h[@]}"; do echo "<$v>"; done | sort
echo "${h[*]}" | tr ' ' '\n' | sort | tr '\n' ' '
echo
echo "${h[@]/f/g}" | tr ' ' '\n' | sort | tr '\n' ' '
echo
h=()
echo "${#h[@]}"
# In arithmetic, the text of the subscript is the key.
h=([x]=1 [y]=2)
echo $((h[x] + h[y]))
k=x
echo $((h[$k] * 10))
: $((h[z] = 5)) $((h[y] += 1))
echo "${h[z]} ${h[y]}"
# `unset 'h[key]'` removes a key.
unset 'h[x]' 'h[nosuch]'
echo "${#h[@]} [${h[x]-unset}]"
# Once it is unset, `h[i]=` makes an indexed array again.
unset h
h[1]=v
echo "${#h[@]} ${h[1]}"
# `read -A` reads pairs of keys and values.
typeset -A r
echo 'k1 v1 k2 v2' | {
  read -A r
  echo "${r[k2]} ${#r[@]}"
}
# In a function, `local -A` and `typeset -A` make local ones.
f() {
  local -A l
  l[a]=1
  typeset -A t=([q]=r)
  echo "${l[a]} ${t[q]}"
}
f
echo "[${l-unset}] [${t-unset}]"
# Like other arrays, they aren't exported.
typeset -A e=([a]=1)
export e
env | grep -c '^e='
# Errors: an odd number of elements, and a mix of `[key]=value` and pairs.
(e=(a); echo never)
(e=([a]=1 b 2); echo never)
# A read-only one can't be changed.
readonly e
(e[a]=2; echo never)
(e+=(b 2); echo never)
echo "${e[a]}"
