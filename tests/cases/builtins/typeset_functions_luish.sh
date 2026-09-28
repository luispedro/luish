# typeset -f where luish differs from zsh and bash: the layout of the
# definitions (luish's own, as for savestate), and the errors.
f() {
    if [ "$1" = x ]; then echo "x: ${2:-none}"; elif [ -n "$1" ]; then :; fi
    while false; do :; done
    case $1 in a|b) echo ab ;; *) ;; esac
    x=1 y=`echo z` cmd >/dev/null 2>&1 <in
    a && b || ! c | d &
    { grp; } 2>/dev/null
    [[ -n $x && $y == z* ]]
    arr=(a 'b c') h[k]=v
    cat <<-'END'
	raw $x
	END
}
g() ( echo in g ) >/dev/null
function h { echo h; }
typeset -f
# -f can't be combined with attributes (zsh ignores most of them, bash
# applies some to the functions), nor define functions (bash's error).
typeset -fx f 2>/dev/null
echo "status $?"
typeset -f f=x 2>/dev/null
echo "status $?"
# -F is zsh's floating point, bash's names of functions: neither is supported.
typeset -F 2>/dev/null
echo "status $?"
# local is a special built-in, without -f (as zsh's local).
(k() { local -f f; echo not reached; }; k) 2>/dev/null
echo "status $?"
