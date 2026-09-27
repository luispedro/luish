# The shell leaves what it inherits as it was: a closed stdout stays closed
# (so writing fails), and an ignored SIGPIPE stays ignored.
$SH -c 'echo hi 2>/dev/null; echo "status $?" >&3' 3>&1 >&-
(trap '' PIPE; $SH -c 'grep SigIgn /proc/self/status')
$SH -c 'grep SigIgn /proc/self/status'
