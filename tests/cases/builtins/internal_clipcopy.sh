# __luish_internal clipcopy without a terminal (the cases run without one):
# an error, before anything is read, so standard input is left for what
# follows. Usage errors have status 2. `clipcopy` itself is a built-in only
# in interactive shells.
err() { sed -n 's/^.*: \(clipcopy: \)/\1/p'; }
echo kept | { __luish_internal clipcopy 2>&1 | err; cat; }
__luish_internal clipcopy 2>/dev/null </dev/null; echo "status $?"
__luish_internal clipcopy a b 2>&1 | err; __luish_internal clipcopy a b 2>/dev/null; echo "status $?"
__luish_internal clipcopy -x 2>&1 | err; __luish_internal clipcopy -x 2>/dev/null; echo "status $?"
command -v clipcopy; echo "status $?"
$SH -ic 'command -v clipcopy' 2>/dev/null
