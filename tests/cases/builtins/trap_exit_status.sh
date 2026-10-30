# `exit` without an argument in a trap action exits with the status from
# before the trap ran (dash's `savestatus`), also in a function, `eval`,
# `$(...)` or subshell run from the action.
$SH -c "trap 'false; exit' EXIT; true"; echo "status $?"
$SH -c "trap 'false; exit' EXIT; (exit 4)"; echo "status $?"
$SH -c "trap 'false; exit' EXIT; exit 5"; echo "status $?"
$SH -c "trap 'false; exit 9' EXIT; true"; echo "status $?"
$SH -c "f() { false; exit; }; trap f EXIT; true"; echo "status $?"
$SH -c "trap 'eval \"false; exit\"' EXIT; (exit 6)"; echo "status $?"
$SH -c "trap 'x=\$(false; exit); echo cs=\$?' EXIT; true"; echo "status $?"
$SH -c "trap '(false; exit); echo sub=\$?' EXIT; true"; echo "status $?"
# In a signal's trap, the status before it is that of the command it ran
# after (`kill`), and the EXIT trap gets that status.
$SH -c "trap '(exit 7); exit' USR1; false; kill -USR1 \$\$; echo no"; echo "status $?"
$SH -c "trap 'echo exit-trap \$?' EXIT; trap 'false; exit' USR1; kill -USR1 \$\$"; echo "status $?"
$SH -c "trap 'trap \"false; exit\" USR2; kill -USR2 \$\$' USR1; kill -USR1 \$\$; echo no"; echo "status $?"
# Outside traps, `exit` takes the last command's status as before.
$SH -c "trap 'false' USR1; kill -USR1 \$\$; (exit 3); exit"; echo "status $?"
# A trap that ends `wait` runs once `wait` is over: `$?` there, and so a bare
# `exit`, is wait's 128+n. The signal is sent once the shell waits.
sleep 10 &
p=$!
trap 'echo "trap: $?"' USR1
(
    until case $(cat /proc/$$/wchan) in do_wait | sigsuspend*) true ;; *) false ;; esac; do :; done
    kill -USR1 $$
) &
wait $p
echo "wait: $?"
kill $p
wait
