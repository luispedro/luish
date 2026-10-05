# bash's forms of `disown`, which zsh lacks: process ids, `-a`, `-r` and
# `-h` (which does nothing, as luish never sends SIGHUP to its jobs).
sleep 10 &
p1=$!
disown $p1
echo "by pid: $?"
wait
kill -0 $p1 && echo "still running"
kill $p1

# `-h` leaves the job in the table, so `wait` waits for it.
sleep 0 &
disown -h
jobs %1 >/dev/null && echo "-h: still a job"
wait

# `-a` removes all jobs, `-r` all the running ones.
sleep 10 &
p1=$!
: | sleep 10 &
p2=$!
disown -a
wait
kill -0 $p1 && kill -0 $p2 && echo "-a: both running"
kill $p1 $p2
sleep 10 &
p1=$!
disown -r
wait
kill -0 $p1 && echo "-r: running"
kill $p1

# A pid that isn't a job's, or isn't a number, is an error; the other jobs
# are still removed.
sleep 10 &
p1=$!
disown 1 x %1 2>/dev/null
echo "errors: $?"
wait
kill -0 $p1 && echo "removed anyway"
kill $p1
# A job that doesn't exist is status 1, as in bash (zsh: 127).
disown %9 2>/dev/null
echo "no such job: $?"
disown -z 2>/dev/null
echo "bad option: $?"
