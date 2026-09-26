# set's lone - and +, unset of a bad name, redefining special built-ins,
# and . on a directory, as in dash.
set - +; echo "$@"
set + -; echo "$@"
set -x -; echo "[$-]"
set - ; echo "$@"
set -- a b; set -; echo "$@"
$SH -c 'unset %; echo notreached' 2>/dev/null; echo "status $?"
$SH -c 'unset -v 1a; echo notreached' 2>/dev/null; echo "status $?"
$SH -c 'unset -f 1a; echo reached' 2>/dev/null; echo "status $?"
for b in eval export exit set unset local : . readonly; do
  $SH -c "$b() { echo f; }; echo notreached" 2>/dev/null; echo "$b status $?"
done
$SH -c 'true() { echo f; }; true; cd() { echo f2; }; cd' 2>/dev/null; echo "status $?"
mkdir -p dir
. ./dir/; echo "status=$?"
$SH -c '. ./nonexistent; echo notreached' 2>/dev/null; echo "status $?"
