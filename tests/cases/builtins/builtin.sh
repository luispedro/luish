# reference: zsh
# `builtin`, as in zsh: runs a built-in, bypassing functions.
builtin; echo "none $?"
builtin echo hi; echo "status $?"
echo() { printf 'function\n'; }
builtin echo bypassed
cd() { printf 'no\n'; }
builtin cd / && builtin pwd
unset -f echo cd
builtin foo; echo "missing $?"
builtin ls; echo "external $?"
builtin builtin echo nested
builtin command echo command
command builtin echo through command
f() { builtin local a=1; echo "local $a"; }
f
builtin export e=exported; sh_e=$(env | grep '^e='); echo "$sh_e"
type builtin
command -v builtin
builtin false; echo "false $?"
# A special built-in stays special: its errors exit the shell.
(builtin set -o bogus; echo not reached) || echo "set failed"
builtin exit 3
echo not reached
