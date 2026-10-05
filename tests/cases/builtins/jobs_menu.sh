# jobs -i (the menu of the jobs) needs a terminal; in dash, -i is an
# illegal option. Both fail with status 2.
jobs -i; echo "status $?"
sleep 5 &
jobs -i %1; echo "status $?"
jobs -i %1 %1; echo "status $?"
jobs -i %9; echo "status $?"
kill $!
wait
