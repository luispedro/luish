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
__luish_internal plugin list
