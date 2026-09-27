# reference: zsh
# `let`, as in zsh: each argument is an arithmetic expression, and the status
# is whether the last one is non-zero.
let x=1+2 'y = x * 3'; echo "status $? x=$x y=$y"
let 0; echo "zero $?"
let 'x - 3'; echo "last zero $?"
let 1 0 5; echo "last non-zero $?"
let -1; echo "negative $?"
let -- z=4; echo "dashes $? z=$z"
let ''; echo "empty $?"
let; echo "none $?"
# An error stops at that argument, and doesn't exit the shell.
let '1 +' w=5; echo "error $? w=$w"
let 'q = 1 / 0'; echo "division $?"
readonly r=1
let r=2; echo "readonly $? r=$r"
n=3
while let 'n > 0'; do let 'n -= 1'; echo "n=$n"; done
let 'n == 0' && echo "done"
type let
command -v let
set -e
let 'n + 1'
let n
echo not reached
