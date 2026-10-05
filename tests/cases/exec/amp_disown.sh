# reference: zsh
# `&|` and `&!` run a command in the background and disown it: `wait`
# doesn't wait for it, but it keeps running.
sleep 10 &|
pid=$!
wait
kill -0 $pid && echo "&| running"
kill $pid

sleep 10 &!
pid=$!
wait
kill -0 $pid && echo "&! running"
kill $pid

# A pipeline, then other jobs that are still waited for.
: | sleep 10 &|
pid=$!
sleep 0 &
wait
kill -0 $pid && echo "pipeline running"
kill $pid

# `&!` before a reserved word, `)`, `;;` or a comment, and `&|` before a
# command.
{ sleep 10 &! }
pid=$!
wait
kill -0 $pid && echo "in braces running"
kill $pid
if true; then sleep 10 &! fi
pid=$!
wait
kill -0 $pid && echo "before fi running"
kill $pid
(sleep 0 &!)
case a in a) sleep 0 &! ;; esac
sleep 0 &! # a comment
sleep 0 &| echo "after &|"
wait
echo done
