# kill and trap parse signals as dash does: names in any case without SIG,
# numbers of digits only, and kill's and trap's options.
kill -l
kill -l 143; kill -l 9; kill -l 0 2>/dev/null; echo "status $?"
kill -l TERM 2>/dev/null; echo "status $?"
kill -l 200 2>/dev/null; echo "status $?"
for s in -terM -SigterM -SIGTERM -9999 -n -0 '-s term' '-s SIGTERM' '-s' -15 '-- '; do
  sleep 5 &
  pid=$!
  kill $s $pid 2>/dev/null; echo "kill $s: $?"
  kill $pid 2>/dev/null
  wait $pid
done
kill 2>/dev/null; echo "status $?"
kill -l 1 2 2>/dev/null; echo "status $?"
kill -0 $$; echo "status $?"
kill -0 abc 2>/dev/null; echo "status $?"
trap 'echo int' int; trap 'echo hup' 1; trap
trap - INT hup; trap
trap - SIGINT; echo "status $?"
trap 'echo x' BOGUS INT 2>/dev/null; echo "status $?"; trap
trap 'echo x' rtmin+1 RTMAX-2 Rtmax; trap; trap - 35 62 64
$SH -c 'trap -p' 2>/dev/null; echo "status $?"
$SH -ec 'trap "echo trap-exit" EXIT; trap -1 EXIT; echo bad' 2>/dev/null; echo "status $?"
$SH -ec 'trap "echo noprint" EXIT; trap 0 EXIT; echo ok0'
$SH -ec 'trap "echo noprint" EXIT; trap 07 EXIT; echo ok07'
