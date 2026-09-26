# As in dash, a built-in whose output can't be written reports an I/O
# error and its status gets bit 1.
echo hi > /dev/full; echo "status=$?"
printf '%s\n' hi > /dev/full; echo "status=$?"
type echo > /dev/full; echo "status=$?"
ulimit -a > /dev/full; echo "status=$?"
pwd > /dev/full; echo "status=$?"
set > /dev/full; echo "status=$?"
alias a=b; alias > /dev/full; echo "status=$?"
printf '%d\n' x > /dev/full 2>/dev/null; echo "status=$?"
command -V cat > /dev/full; echo "status=$?"
kill -l > /dev/full; echo "status=$?"
