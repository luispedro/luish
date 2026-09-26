set -e
false || echo or-ok
if false; then :; fi
! true
false && true
while false; do :; done
echo still-running
f() { false; echo not-reached; }
f || echo f-failed
(false); echo not-here
