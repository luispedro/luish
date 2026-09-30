# `print` is a built-in only in interactive shells, so scripts find the
# same commands as in dash; `__luish_internal print` is the same command
# in any shell.
(PATH=/nonexistent; print x 2>/dev/null)
echo "status $?"
p() { __luish_internal print "$@"; }
p -l a "b\tc"
# -P expands `%` sequences whether or not `prompt.percent` is on.
p -P '[%~] %% %(?.ok.bad) %F{2}g%f'
p -P '%[bogus]x' 2>/dev/null
# Unlike zsh, where they end the shell, a bad name or a readonly variable
# for -v is an error with status 1.
p -v 1x a 2>/dev/null; echo "status $?"
readonly r=1
p -v r a 2>/dev/null; echo "status $? $r"
# -S isn't supported, and -p has no coprocess to write to.
p -S x 2>/dev/null; echo "status $?"
p -p x 2>/dev/null; echo "status $?"
# -s and -z do nothing without a line editor.
p -s x; p -z y; echo "status $?"
# Code points that aren't characters give nothing.
p '\UD800\U110000|'
p -u 3 fd3 3>f; cat f
p -f '%d\n' x 2>/dev/null; echo "status $?"
# The errors are reported with the command's name.
p -q 2>&1 | sed 's/.*: __luish/__luish/'
__luish_internal help print | head -n 1
