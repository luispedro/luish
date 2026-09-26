# The last command of a subshell or a background job replaces the forked
# shell process instead of being forked again, unless a trap needs it.
($SH -c 'echo $PPID > parent') &
wait
[ "$(cat parent)" = "$$" ] && echo "subshell in background: exec'd"
{ :; $SH -c 'echo $PPID > parent'; } &
wait
[ "$(cat parent)" = "$$" ] && echo "last in group: exec'd"
{ $SH -c 'echo $PPID > parent'; :; } &
wait
[ "$(cat parent)" != "$$" ] && echo "not last: forked"

(trap 'echo subshell trap' EXIT; /bin/echo in subshell)
(if true; then /bin/echo in if; fi)
({ /bin/echo in braces; })
(/bin/echo redirected > out); cat out
( (/bin/echo nested subshell) )
(false || /bin/echo after or)
(! /bin/false && echo negated)
(/bin/sh -c "exit 5"); echo "status: $?"
x=$(/bin/echo in substitution); echo "$x"
