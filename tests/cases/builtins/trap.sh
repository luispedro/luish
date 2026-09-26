trap 'echo exit trap $?' EXIT
trap 'echo usr1' USR1
kill -USR1 $$
echo after
trap
trap - USR1
( trap 'echo sub-exit' EXIT; echo in-sub )
x=$(trap 'echo cs-exit' EXIT; echo cs)
echo "$x"
exit 3
