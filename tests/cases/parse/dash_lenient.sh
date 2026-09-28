# As in dash: a bad ${...} is an error only when expanded (so bash-only
# forms in a branch not taken are fine), a function body can be any
# command, and $( in a here-doc delimiter is a syntax error.
if test -f /; then
  echo ${%} ${x//a/b} ${x:0:1} ${!x} ${x^^} ${}
else
  echo ok
fi
v=abc
[ -n "$BASH_VERSION" ] && echo "${v^^}"
$SH -c 'v=abc; echo "${v^^}"; echo notreached' 2>/dev/null; echo "status $?"
$SH -c 'echo ${%}; echo notreached' 2>/dev/null; echo "status $?"
$SH -c 'x=${x:h}; echo notreached' 2>/dev/null; echo "status $?"
f() echo hi
f
g() h() { echo in; }
g; h
k() x=1; k; echo "$x"
m() >/dev/null; m; echo "status $?"
n() ! false; n; echo "status $?"
printf 'cat <<$(a)\nhere\n$(a)\n' | $SH 2>/dev/null; echo "status $?"
cat <<`a`
here
`a`
cat <<"$(a)"
here2
$(a)
