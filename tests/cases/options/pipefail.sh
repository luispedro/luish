# reference: zsh
# set -o pipefail (POSIX 2024; Debian's dash lacks it): a pipeline's status
# is that of its last command that failed, or 0 if none did.
false | true; echo "off $?"
set -o pipefail
false | true; echo "on $?"
(exit 3) | (exit 2) | true; echo "rightmost $?"
(exit 3) | true | (exit 4); echo "last $?"
true | true; echo "none $?"
false; echo "single $?"
! false | true; echo "negated $?"
! true | true; echo "negated true $?"
{ sh -c 'kill -9 $$' | true; } 2>/dev/null; echo "signal $?"
# In conditions, subshells and command substitutions.
if false | true; then echo then; else echo "else $?"; fi
false | true || echo "or $?"
false | true && echo not reached
(false | true); echo "subshell $?"
x=$(false | true); echo "cmdsubst $?"
x=$(set +o pipefail; false | true); echo "cmdsubst off $?"
while false | true; do echo not reached; done
# By name for setopt, as zsh's PIPE_FAIL, and on the command line.
set +o pipefail
false | true; echo "off again $?"
setopt pipe_fail
false | true; echo "setopt $?"
unsetopt pipefail
$SH -o pipefail -c 'false | true'; echo "command line $?"
$SH -c 'set -o pipefail; set +o' | grep pipefail
# With set -e, a failed pipeline exits.
$SH -ec 'set -o pipefail; false | true; echo not reached'; echo "errexit $?"
$SH -ec 'set -o pipefail; ! false | true; echo negated ok'; echo "errexit negated $?"
case $- in *pipefail*) echo letter;; esac
