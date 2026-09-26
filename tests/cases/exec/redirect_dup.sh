# n>&n does nothing, even if n is closed (as in dash).
: 3>&3
echo hello 4>&4
exec 5>&-
echo "status $?" 5>&5
# A target that is not a number or - is a fatal syntax error.
$SH -c 'echo one 1>&nonexistent-filename; echo "status=$?"' 2>/dev/null
echo "exit $?"
$SH -c 'echo one 1>&1x; echo notreached' 2>/dev/null
echo "exit $?"
