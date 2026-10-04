# Variable tracing through `local` and temporary assignments: after a value
# is put back, `where` shows where it was set (or unset); `where -a` shows
# that it was put back. A function's `typeset` (unlike `local`) unsets.
cat > r.sh <<'X'
X=hello
g() { where X; }
X=tmp g
X=tmp true
where X
where -a X
f() { local X=inner; h; where X; }
h() { local X; where X; X=deeper; }
f
where X
where -a X
unset X
f2() { local X=in; }
f2
where X
where -a X
X=1
f3() { typeset X; where X; }
f3
where X
X
$SH -o vars.trace ./r.sh
echo "-- history"
$SH -o vars.trace_history ./r.sh
