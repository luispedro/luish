# reference: zsh
# Without a current job, `disown` fails.
disown 2>/dev/null
echo "no current job: $?"

# `disown` removes a job from the table: `wait` no longer waits for it, but
# it keeps running.
sleep 10 &
pid=$!
disown
echo "disown: $?"
wait
echo "wait: $?"
kill -0 $pid && echo "still running"
kill $pid

# A job named by its number, among others.
sleep 10 &
p1=$!
sleep 0 &
disown %1
echo "disown %1: $?"
wait
kill -0 $p1 && echo "job 1 still running"
kill $p1
# (Jobs started without job control have no command text, as in dash, so
# `%sleep` can't name them.)
