# `__luish_internal savestate` sets syntax.repeat before the functions, so
# that functions with a `repeat` loop read back.
setopt syntax.repeat
f() { repeat 2 { echo hi }; }
__luish_internal savestate > state
$SH -c '. ./state; f'
grep -e syntax.repeat -e '^f()' state
