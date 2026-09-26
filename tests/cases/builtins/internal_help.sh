# `__luish_internal help` is `help` in any shell: the list of built-ins,
# the help for each name given, and status 1 for a name without help.
__luish_internal help | head -n 3
__luish_internal help true false
echo "status $?"
__luish_internal help true nosuch
echo "status $?"
