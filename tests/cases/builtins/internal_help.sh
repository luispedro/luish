# `__luish_internal help` is `help` in any shell: the list of built-ins,
# the help for each name given (also `[[`), and status 1 for a name without
# help.
__luish_internal help | head -n 3
__luish_internal help true false
echo "status $?"
__luish_internal help true nosuch
echo "status $?"
# `[[` is a reserved word, listed after the built-ins, with its own page.
__luish_internal help | tail -n 3
__luish_internal help '[[' | head -n 1
