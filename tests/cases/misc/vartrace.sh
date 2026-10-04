# Variable tracing (`vars.trace`): `where` tells where each variable was last
# set: the file and line, with the function running, or the -c command or
# standard input. Values are shown quoted (control characters as $'...').
cat > lib.sh <<'X'
X=hello
addpath() {
  PATH=$1:$PATH
}
X
cat > s.sh <<'X'
. ./lib.sh
f() {
  local X=inner
  where X
}
f
where X
addpath /opt/bin
# `path` is `PATH`.
where PATH path
for i in a b; do :; done
read r <<E
line
E
getopts ab: o -b val
: ${d:=default} $((n = 5))
cd /
cd "$HOME"
eval 'V=ev'
a=(1 "two words") q='say "$x" `c`\' e=$(printf 'bold\033[1m\n')
typeset -A h
h[k]=v
where i r o OPTARG OPTIND d n OLDPWD V a q e h
unset X
where X
where Z; echo "not set: $?"
where 1bad; echo "bad name: $?"
x=1 where x
where x; echo "after: $?"
X
env PATH=/usr/bin:/bin $SH -o vars.trace ./s.sh 2>&1 | sed "s|$HOME|HOME|"
# Where it was first set: in the environment (with the value it still has),
# by luish as it started (with `-o vars.trace`), or before tracing began
# (with `setopt`).
env -i HOME="$HOME" A=env B=env $SH -o vars.trace -c 'B=changed; where A B IFS PS2 SHLVL'
env -i HOME="$HOME" $SH -c 'early=1; setopt vars.trace; where early IFS'
# The -c command and standard input.
$SH -o vars.trace -c 'echo
Y=1
where Y'
printf 'echo\nY=1\nwhere Y\n' | $SH -o vars.trace
# Without tracing, `where` is not a built-in (except in interactive
# shells); `__luish_internal where` fails. `unsetopt` stops tracing.
$SH -c 'where X' 2>/dev/null; echo "where: $?"
$SH -c '__luish_internal where X' 2>&1; echo "internal: $?"
$SH -o vars.trace -c 'X=1; unsetopt vars.trace; __luish_internal where X' 2>/dev/null; echo "unsetopt: $?"
# Tracing in a subshell stays there.
$SH -c '(setopt vars.trace; X=1; where X); __luish_internal where X' 2>/dev/null; echo "subshell: $?"
# At the prompt (of an interactive shell, here without a terminal).
printf 'X=1\nwhere X\n' | $SH -i +m -o vars.trace 2>/dev/null
