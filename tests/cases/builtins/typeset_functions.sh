# reference: zsh
# typeset -f prints function definitions that read back, +f their names.
f() {
    if [ "$1" = x ]; then
        echo "x: ${2:-none}" 'a  b'
    fi
    for i in 1 2; do printf '%s,' "$i"; done
    case $1 in a|b) echo ab ;; *) echo other ;; esac
    cat <<EOF
body $1
EOF
    echo $(echo sub) $((1 + 2)) | cat
}
g() ( echo in g ) >/dev/null
h() { echo h; } 2>&1
typeset +f
typeset +f f nosuch g
echo "status $?"
typeset -f nosuch
echo "status $?"
typeset -f f >/dev/null
echo "status $?"
declare -f g >/dev/null
echo "status $?"
# The definitions read back.
defs=$(typeset -f f g h)
unset -f f g h
typeset +f
echo "status $?"
eval "$defs"
f x
f a
g
h
# All of them, under other names.
eval "$(typeset -f | sed 's/^\([fgh]\)/\1_copy/')"
typeset +f
f_copy b
# In a function, nothing is made local.
k() { typeset -f k >/dev/null; typeset +f k; }
k
