# reference: zsh
# zsh's parameter expansion flags, ${(flags)name...}: a bad substitution
# in dash. The flags apply in zsh's order (the operator, then j, s, the
# case, u and the order), whatever order they are written in.
p() { printf '<%s>' "$@"; echo; }
a=(b a c a)
# Joining and splitting.
p "${(j:,:)a[@]}" "${(j.-.)a[*]}" "${(j(+))a[@]}" "${(j::)a[@]}" ${(j:, :)a[@]}
p "${(F)a[@]}"
x='x,y,,z'
p ${(s:,:)x}
# Quoted, only the first and the last word can be empty (all with @).
p "${(s:,:)x}" "${(@s:,:)x}"
p ${(s:,,:)x} ${(s::)x}
for x in ',,a' 'a,,' ',a,,b,' ',,' ','; do p "${(s:,:)x}"; done
x=$(printf 'one\ntwo words\n\nthree')
p ${(f)x}
p "${(f)x}"
# Unquoted, the words of s aren't split again (but are globbed); other
# results are.
touch f1 f2
x='f*,g h'
p ${(s:,:)x}
p ${(U)x}
b=("x y" z)
p ${(j:,:)b[@]} ${(@)b[@]}
# s joins the elements first (with the first character of IFS).
b=(x y,z)
p ${(s:,:)b[@]} ${(j:-:s:,:)b[@]}
(IFS=-; p "${(s:,:)b[@]}")
# Case.
x='hELLO wORLD 3abc a_b foo-bar'
p "${(L)x}" "${(U)x}" "${(C)x}"
p ${(U)a[@]}
# Unique and order (also case-insensitive, numeric, reverse, the array's).
p ${(u)a[@]}
p ${(o)a[@]}
p ${(O)a[@]}
p ${(uo)a[@]}
p ${(Oa)a[@]}
p ${(a)a[@]}
c=(b B a A C)
p ${(o)c[@]}
p ${(oi)c[@]}
p ${(Oi)c[@]}
p ${(i)c[@]}
n=(10 9 100 x10 x9 a01 a1 a001 a01b10 a1b9)
p ${(n)n[@]}
p ${(On)n[@]}
# Order after joining, splitting and case.
p "${(oj:,:)a[@]}" ${(os:,:)x} ${(oL)c[@]}
x='c,a,b'
p ${(os:,:)x} ${(s:,:O)x}
# Associative arrays: keys, values, both.
typeset -A h
h=(k2 v2 k1 v1)
p ${(ko)h[@]} ${(vo)h[@]} ${(o)h[@]}
set -- ${(kv)h[@]}
echo $#
p "${(U)h[k1]}" ${(kvU)h[k2]}
# k and v are ignored for other arrays and strings.
p ${(k)a[@]} ${(kv)a[@]}
x=s
p ${(k)x}
# Quoted: [*] and $* are joined first, unless there is @ or j.
p "${(o)a[@]}" "${(o)a[*]}" "${(@o)a[*]}" "${(oj:,:)a[*]}"
set -- b a
p ${(o)@} ${(o)*} "${(o)@}" "${(o)*}" "${(@o)*}" "${(j:,:)*}"
p "${(@)a}" "${(@)a[0]}"
# With operators, which come first, and subscripts.
x=abc
p ${(U)x:-def} ${(U)nosuch:-def} ${(U)x#a} ${(U)x/b/x} ${(U)x:1} ${(C)x}
p ${(j:,:)a[@]/a/y} ${(U)a[@]%a} "${(j:,:)a[@]:1:2}"
p ${(U)nosuch:=assigned} "$nosuch" ${(U)x+alt}
# Around other text.
p "pre${(o)a[@]}post" pre${(o)a[@]}post "pre${(j:,:)a[@]}post"
# Empty and unset.
e=()
set -- "${(o)e[@]}"; echo $#
set -- "${(j:,:)e[@]}"; echo $#
set -- "${(U)e[@]}"; echo $#
set -- "${(s:,:)unset}"; echo $#
set -- ${(s:,:)unset}; echo $#
set -- "${(U)unset}"; echo $#
set -- ${(U)unset}; echo $#
x=
set -- "${(s:,:)x}"; echo $#
x=',a,'
set -- ${(s:,:)x}; echo $#
# Where the result is one word, arrays are joined first, and s doesn't
# split.
x=${(o)a[@]}; p "$x"
x="${(o)a[@]}"; p "$x"
x=${(j:-:)a[@]}; p "$x"
y='c,b'
x=${(os:,:)y}; p "$x"
case ${(o)a[@]} in 'b a c a') echo joined first ;; *) echo sorted ;; esac
cat <<EOF
${(o)a[@]} ${(j:-:)a[@]} ${(U)a[*]}
EOF
# set -u
(set -u; echo ${(U)unset}; echo not reached) 2>/dev/null || echo failed
(set -u; echo ${(U)unset-d})
