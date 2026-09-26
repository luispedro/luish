# reference: zsh
# `source` follows zsh: the current directory first, then PATH, and
# arguments become the positional parameters while the file runs.
mkdir d sub
echo 'echo cwd' > f
echo 'echo "path $# $*"; set -- changed; return 3; echo never' > d/f
echo 'echo only-path' > d/g
echo 'echo in-sub' > sub/h
PATH=$PWD/d:$PATH
source f
source g
set -- x y
source d/f a b
echo "status $? args $*"
source ./d/f
echo "status $? args $*"
. f
v=1 source f
echo "v=$v"
cd sub && source h && cd ..
f() { source ./d/f "$@"; echo "in function $?: $*"; }
f p q
