x=1; (x=2; echo in $x); echo out $x
(cd /; pwd); pwd >/dev/null
f() { echo f; }; (unset -f f; f 2>/dev/null || echo gone); f
