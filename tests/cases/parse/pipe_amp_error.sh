# Syntax errors with `|&` (in dash, `|&` always is): nothing after it, or a
# function definition before it.
$SH -c 'echo a |&' 2>/dev/null; echo "status $?"
$SH -c 'f() { :; } |& cat' 2>/dev/null; echo "status $?"
$SH -c 'echo a |& |& cat' 2>/dev/null; echo "status $?"
