# Where `for ((` follows bash rather than zsh: more than three expressions
# are an error (zsh runs the loop), and `set -x` shows each expression
# evaluated as `(( expr ))`, after expansion (zsh shows the bare text, and
# not an empty condition).
(eval 'for ((a; b; c; d)); do echo body; break; done') 2>/dev/null || echo "four expressions"
exec 2>&1
n=2
set -x
for (( i = 0 ; i < $n ; i++ )); do :; done
for ((;;)); do break; done
set +x
