# ulimit is a port of dash's.
ulimit -a | sed 's/ [0-9][0-9]*$/ N/'
ulimit -a 42 2>/dev/null; echo "status $?"
ulimit 1 2 2>/dev/null; echo "status $?"
ulimit 1x 2>/dev/null; echo "status $?"
ulimit -q 2>/dev/null; echo "status $?"
(
  ulimit -f; ulimit -H -f
  ulimit -f $(( (1 << 62) + 1 )); ulimit -f
)
(
  ulimit -S -t 123456; ulimit -S -t; ulimit -H -t
  ulimit -H -t 123457; ulimit -S -t; ulimit -H -t
  ulimit -t 123455; ulimit -S -t; ulimit -H -t
  ulimit -S -t 123454; ulimit -t; ulimit -H -t
  ulimit -S -t 123460 2>/dev/null; echo "status $?"
  ulimit -tS; ulimit -St
)
ulimit -c 0; ulimit -c
