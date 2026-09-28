# `dirstack` is tied to the directory stack only by array assignments
# (dirstack_tied.sh): a dash script can use it as an ordinary variable.
dirstack=/tmp/x
echo "$dirstack"
unset dirstack
echo "[$dirstack]"
dirstack=a
dirstack=$dirstack:b
echo "$dirstack"
f() {
    local dirstack
    echo "[$dirstack]"
    dirstack=/opt
    echo "$dirstack"
}
unset dirstack
f
g() {
    local dirstack=/srv
    echo "$dirstack"
}
g
for dirstack in 1 2; do :; done
echo "$dirstack"
read dirstack <<END
/from/read
END
echo "$dirstack"
dirstack=/tmp/y $SH -c 'echo "$dirstack"'
dirstack=/tmp/z env | grep '^dirstack='
echo "$dirstack"
