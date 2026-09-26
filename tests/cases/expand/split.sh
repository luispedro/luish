show() { printf '<%s>' "$@"; echo; }
x="  a   b  "; show $x; show "$x"
IFS=:; y="a::b:"; show $y
IFS=' :'; y=" : a : : b "; show $y
IFS=; z="a b"; show $z
unset IFS; w=" a	b
c "; show $w
IFS=:; v=a:b; show $v "$v" x${v}y
e=; show $e "$e" ${e} x${e}
show ${unset_var} "${unset_var}"
IFS=x; show ${u-axbxc}
