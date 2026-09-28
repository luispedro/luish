# zsh's `path`: until it is assigned a string (path_ordinary.sh), it is the
# array of the directories in PATH, and an array assignment sets PATH.
PATH=/usr/bin:/bin
echo "$path ${#path[@]} ${path[1]} ${!path[@]}"
for d in "${path[@]}"; do echo "[$d]"; done
PATH=a::b:
echo "${#path[@]} [${path[1]}] [${path[3]}]"
PATH=
echo "${#path[@]}"
path=(/usr/bin "/a b")
echo "$PATH"
path+=(/c)
echo "$PATH"
path[1]=/d
path[-1]+=/e
path[4]=/f
echo "$PATH"
unset 'path[0]'
echo "$PATH"
PATH=/bin:/usr/bin
path=("${path[@]}" /g)
echo "$PATH"
# In effect for commands.
path=(/nowhere)
cat /dev/null 2>/dev/null || echo not found
path=(/usr/bin /bin)
cat /dev/null && echo found
# Only for the command, when put before it.
path=(/tmp) $SH -c 'echo "$PATH"'
echo "$PATH"
# `read -A`, and `typeset -a`, which keeps it.
read -A path <<END
/h /i
END
echo "$PATH"
typeset -a path
echo "$PATH ${path[1]}"
typeset -A path 2>/dev/null || echo not associative
# A local `path` is ordinary; the tie is back after the function.
f() {
    local path=(/j)
    echo "$PATH ${path[@]}"
}
f
path=(/usr/bin /bin /k)
echo "$PATH"
# So is one before a command.
path=x true
path+=(/l)
echo "$PATH"
# A string assignment makes it ordinary, as does unset.
path=/m
path=(/n /o)
echo "$PATH ${path[@]}"
unset path
path=(/p)
echo "$PATH ${path[@]}"
# Read-only.
PATH=/usr/bin:/bin
readonly path
(path=(/q)) 2>/dev/null || echo read-only
echo "$PATH"
