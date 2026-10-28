# reference: zsh
# `source` skips a first `--`, before the file and its arguments.
echo 'echo "f $# $*"' > f
set -- x y
source -- f
source -- ./f a b
echo "args $*"
source -- f --
# Unlike `.`, any other word starting with `-` is a file name (as in zsh).
echo 'echo dash-x' > ./-x
source -x
