# Tab after `${name[` completes the array's indices, or an associative
# array's keys, each with its element as the description, and closes the
# expansion. Keys are escaped as in a subscript.
typeset -A h
h=([apple]=red [a\]b]=x ['a b']=y ['$x']=z)
a=(first second)
s=string
c() {
    echo "--- $1"
    __luish_internal complete "$1" | sort
}
c 'echo ${h[ap'
c 'echo ${h['
c 'echo "${h[a'
c 'echo ${h[a\]'
c 'echo ${a['
c 'echo ${s['
c 'echo ${nosuch['
# Special arrays too.
c 'echo ${pipestatus['
# Indices are in numeric order, not sorted as text.
l=(a b c d e f g h i j k l)
__luish_internal complete 'echo ${l['
__luish_internal complete 'echo ${l[1'
