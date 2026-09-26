# set -e applies inside $(...) even when the command is a condition (dash
# runs the substitution with fresh flags).
set -e
if echo $(echo 1; false; echo 2); then
  echo A
fi
x=$(false; echo no) || echo "failed $?"
while echo $(echo w; false; echo x) >/dev/null; do break; done
echo done
