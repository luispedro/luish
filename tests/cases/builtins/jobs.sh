# The job table without job control, as dash keeps it: no command text,
# newest job first, finished jobs shown once, and slots of jobs that `wait`
# has reported reused by the next job.
sleep 10 & p1=$!
: | sleep 10 & p2=$!
(exit 3) & p3=$!
wait $p3; echo "wait: $?"
jobs
echo "again:"; jobs
jobs %1 %2
jobs %9; echo "no such job: $?"
jobs -x; echo "bad option: $?"
kill $p1 $p2; wait; echo "wait all: $?"
jobs

# `wait` marks finished jobs; the next job reuses the first such slot.
: & : & wait
sleep 10 & p=$!
jobs
kill $p; wait $p; echo "killed: $?"
jobs

# A foreground command also frees a slot that `wait` has reported.
(exit 4) & wait $!; echo "status: $?"
/bin/true
jobs; echo "empty: $?"

# `wait` for a pid that isn't a child, or for an unknown job.
wait 1; echo "not a child: $?"
wait %5; echo "unknown job: $?"
wait x; echo "not a number: $?"

# `fg` and `bg` need job control.
fg; echo "fg: $?"
sleep 10 & p=$!
bg %1; echo "bg: $?"
fg %1; echo "fg: $?"
kill $p; wait $p
