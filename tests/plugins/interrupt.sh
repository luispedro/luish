# SIGINT stops plugin code (status 130), and a trap for it then runs.
trap 'echo INT trapped' INT
cat > loop.rhai <<'P'
// The signal arrives while the loop runs.
sh::run("(sleep 0.1; kill -INT $$) &");
let n = 0;
loop { n += 1; }
P
__luish_internal plugin load ./loop.rhai
echo "status $?"
__luish_internal plugin list-loaded
# The same in a built-in, and in one waiting in sh::read_line.
cat > spin.rhai <<'P'
sh::builtin("spin", |argv| { loop {} });
sh::builtin("wait_line", |argv| sh::read_line());
P
__luish_internal plugin load ./spin.rhai
(sleep 0.1; kill -INT $$) &
spin
echo "spin: $?"
(sleep 0.1; kill -INT $$) &
mkfifo fifo
sleep 10 > fifo &
writer=$!
wait_line < fifo
echo "wait_line: $?"
kill $writer
