# `path` is tied to PATH only by array assignments (path_tied.sh): a dash
# script can use it as an ordinary variable, which leaves PATH alone.
PATH=/usr/bin:/bin
path=/tmp/x
echo "$path $PATH"
unset path
echo "[$path] $PATH"
path=a:b
path=$path:c
echo "$path $PATH"
f() {
    local path
    echo "[$path]"
    path=/opt
    echo "$path $PATH"
}
unset path
f
g() {
    local path=/srv
    echo "$path $PATH"
}
g
for path in 1 2; do :; done
echo "$path $PATH"
read path <<END
/from/read
END
echo "$path $PATH"
path=/tmp/y $SH -c 'echo "$path $PATH"'
path=/tmp/z env | grep '^path='
echo "$path $PATH"
