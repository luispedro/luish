# As in dash, a signal with a trap ends `read`, with status 1: the variables
# get what was read so far, and the trap runs after `read`.
mkfifo p
exec 3<>p
trap 'echo "trap: x=$x y=$y"' USR1
printf 'ab c' >&3
# The signal is sent once the shell waits for more of the line.
(
    until case $(cat /proc/$$/wchan) in *pipe_read) true ;; *) false ;; esac; do :; done
    kill -USR1 $$
) &
read x y <&3
echo "st=$? x=$x y=$y"
wait
