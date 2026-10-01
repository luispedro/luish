# The startup cache saves every variable the files assign, also those set
# to the value they had when the cache was built, so that a shell started
# without them gets them.
mkdir -p .config/luish/rc.d
cat > .config/luish/rc.d/a.lsh <<'X'
export FOO=bar
N=1
cd /; cd "$HOME"
X
FOO=bar N=1 $SH -i -c 'echo "$FOO $N"' 2>/dev/null
env -i HOME="$HOME" $SH -i -c 'echo "${FOO-unset} ${N-unset}"; env | grep ^FOO=' 2>/dev/null
# `cd` there and back leaves the directory alone.
cd /tmp
env -i HOME="$HOME" $SH -i -c 'pwd' 2>/dev/null
