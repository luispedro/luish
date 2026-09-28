# bash's indirection: ${!x} is the parameter named by the value of x (a
# variable, name[index], a positional or a special parameter), with any
# operator applied to it. zsh's sh emulation doesn't have it (native zsh
# has ${(P)x}).
x=y y=hello
echo "${!x}" "${!x#h}" "${!x%l*}" "${!x:1:3}" "${!x/l/L}" "${!x:+set}"
f() { echo "$1=${!1}"; }
f y
unset y
echo "[${!x}] ${!x-unset} ${!x:-null}"
echo "${!x:=assigned}" "$y"
y=
echo "[${!x-unset}] ${!x:-null}"
(echo "${!x:?empty}") 2>/dev/null || echo "status $?"
# Elements and lists.
a=(p q r)
x='a[1]'; echo "${!x}"
x='a[1+1]'; echo "${!x}"
x='a[-1]'; echo "${!x}" "${!x:=z}"
x='a[@]'; printf '<%s>' "${!x}" "${!x#p}"; echo
x='a[*]'; IFS=-; echo "${!x}"; unset IFS
typeset -A h
h=([k]=v ['a b']=w)
x='h[k]'; echo "${!x}"
x='h[a b]'; echo "${!x}"
# Through an element: ${!name[index]}.
refs=(y a)
y=1
echo "${!refs[0]}" "${!refs[1]}" "${!refs}"
# The index is only evaluated, not expanded (bash expands it).
i=1
x='a[i]'; echo "${!x}"
x='a[$(echo 1)]'; (echo "${!x}") 2>/dev/null || echo "status $?"
# Positional and special parameters.
set -- one two three four five six seven eight nine ten
x=1; echo "${!x}"
x=10; echo "${!x}"
x=0; [ "${!x}" = "$0" ] && echo '$0'
x='#'; echo "${!x}"
x='@'; printf '<%s>' "${!x:8}"; echo
set --
x='@'; set -- "${!x}"; echo "$#"
x='a[@]'; e=(); a=(); set -- "${!x}"; echo "$#"
x=y; set -- "${!x}"; echo "$#"
x='?'; false; echo "${!x}"
# Errors: an unset or invalid reference.
for v in '' 'a b' '1+2' 'a[]' 'y}' '#y' '!y'; do
  x=$v
  (echo "${!x}") 2>/dev/null || echo "[$v] status $?"
done
unset x
(echo "${!x:-default}") 2>/dev/null || echo "unset status $?"
(echo "${#!x}") 2>/dev/null || echo "length status $?"
(set -u; x=nosuch; echo "${!x}") 2>/dev/null || echo "nounset status $?"
# ${!prefix@} and ${!prefix*}: the names of the set variables that start
# with the prefix, sorted.
unset y
yb=1 ya=2 yc=
export ynone
printf '<%s>' "${!y@}"; echo
IFS=,; echo "${!y*}"; unset IFS
set -- "${!zzz@}"; echo "$#"
g() { local yl=1; echo ${!y*}; }
g
echo ${!y*}
# How they are printed back.
h() { echo ${!x} ${!1:-d} ${!a[1]#p} ${!x@} "${!x*}" ${!a[@]}; }
typeset -f h
