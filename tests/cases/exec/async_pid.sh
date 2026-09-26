# `$!` is the background process itself, not a subshell around it, so
# signalling it reaches the command.
$SH -c 'echo $$ > pid' &
wait
[ "$(cat pid)" = "$!" ] && echo "simple command: same pid"

# For a pipeline, `$!` is its last process.
echo x | $SH -c 'cat >/dev/null; echo $$ > pid' &
wait
[ "$(cat pid)" = "$!" ] && echo "pipeline: last pid"

sleep 10 & kill $!; wait $!; echo "killed: $?"
: | sleep 10 & kill $!; wait $!; echo "pipeline killed: $?"
wait
