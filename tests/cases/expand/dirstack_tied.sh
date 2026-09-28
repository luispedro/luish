# zsh's `dirstack`: until it is assigned a string (dirstack_ordinary.sh), it
# is the array of the directory stack (without the current directory), and
# an array assignment replaces the stack.
d() { dirs | sed "s|$HOME|~|g"; }
mkdir a b c
echo "[${dirstack-unset}] ${#dirstack[@]}"
cd a
pushd ../b >/dev/null
pushd ../c >/dev/null
echo "${dirstack#"$HOME"} ${#dirstack[@]} ${dirstack[1]#"$HOME"} ${!dirstack[@]}"
for x in "${dirstack[@]}"; do echo "[${x#"$HOME"}]"; done
popd >/dev/null
echo "${dirstack[@]#"$HOME"}"
dirstack=("$HOME/b" "$HOME/c")
d
dirstack+=(/x)
d
dirstack[1]=/y
dirstack[-1]+=z
dirstack[4]=/w
d
unset 'dirstack[1]'
d
# The directories aren't checked until they are used.
dirstack=(/nonexistent "$HOME/c")
popd 2>/dev/null || echo cannot popd
d
dirstack=("$HOME/b")
popd >/dev/null
d
echo "${PWD#"$HOME"} [${dirstack-unset}]"
# `read -A`, and `typeset -a`, which keeps it.
read -A dirstack <<END
/r /s
END
d
typeset -a dirstack
echo "${#dirstack[@]}"
typeset -A dirstack 2>/dev/null || echo not associative
# Only for the command, when put before it.
dirstack=(/t) d
d
# A local `dirstack` is ordinary; the tie is back after the function.
f() {
    local dirstack=(/u)
    d
    echo "${dirstack[@]}"
}
f
dirstack=(/v)
d
# So is one before a command.
dirstack=x true
dirstack+=(/w)
d
# A string assignment makes it ordinary, as does unset.
dirstack=/m
pushd "$HOME/a" >/dev/null
echo "$dirstack"
d
unset dirstack
dirstack=(/n)
d
echo "${dirstack[@]}"
