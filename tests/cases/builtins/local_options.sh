# `local -` makes the options set with `set` local to the function: they are
# restored when it returns, as in dash.
f() { local -; set -f -u; echo "in: $-"; }
f
echo "after: $-"

# Options changed before `local -` are kept, and a second `local -` doesn't
# save them again.
g() { set -C; local -; set +C -f; local -; set -u; }
g
echo "g: $-"
set +C

# Nested functions restore their own.
inner() { local -; set -x; }
outer() { local -; set -f; inner; echo "outer: $-"; }
outer 2>/dev/null
echo "nested: $-"

# A function without `local -` changes the caller's options, even when
# called from one with it.
h() { set -f; }
k() { local -; h; echo "k: $-"; }
k
echo "k after: $-"
set +f

# Restored also when the function returns early or with an error status.
r() { local -; set -f; return 3; echo not reached; }
r
echo "r: $? $-"

# Other variables can be made local with it.
x=1
m() { local - x=2; set -f; echo "m: $x $-"; }
m
echo "m after: $x $-"

# `set -e` inside is undone too.
e() { local -; set -e; }
e
false
echo "still here: $-"
