# `$(...)` of a built-in that only prints (`echo`, `printf`, `pwd`,
# `command -v`, `type`) runs without a subshell (`exec/utility.rs`); it must
# look as if it ran in one.

x=$(echo a b); echo "[$x]"
x=$(echo -n 'a\tb\n\n'); echo "[$x]"
set -- 'one two' three
printf '[%s]' $(echo "$@") "$(echo "$*")" $(printf '%s\n' "$@"); echo
x=$(printf '%03d|%s\n\n\n' 7 "$1"); echo "[$x]"
x=$(printf 'a\0b\n'); echo "[$x]"
x=$(printf '%s-' ${#1} "${1%% *}" "${2#th}" ~); echo "[$x]" | sed "s|$HOME|HOME|"
false; x=$(echo $?); echo "[$x] $?"
x=$(printf '%d' abc 2>/dev/null); echo "[$x] status $?"
x=$(printf 2>/dev/null); echo "[$x] status $?"
x=$(printf -x 2>/dev/null); echo "[$x] status $?"
x=$(echo); echo "[$x] status $?"

mkdir -p real/sub && ln -s real link
cd link/sub
echo "$(pwd)" | sed "s|$HOME|HOME|"
echo "$(pwd -P)" | sed "s|$HOME|HOME|"
cd "$HOME"

# command -v and type: the cache (hash) is as a subshell leaves it.
f() { :; }
alias ll='ls -l'
for n in cd export f ll if ls nope; do
    x=$(command -v $n); echo "$n [$x] $?"
    x=$(command -V $n 2>/dev/null); echo "$n [$x] $?"
    x=$(type $n 2>/dev/null); echo "$n [$x] $?"
done
x=$(command -pv cat); echo "[$x] $?"
hash | grep -c -e /ls -e /cat
x=$(command echo hi); echo "[$x]"
x=$(command -- echo hi); echo "[$x]"
x=$(command); echo "[$x] $?"

# `2>/dev/null` is undone afterwards.
{ x=$(command -V nope 2>/dev/null); echo "restored [$x]" >&2; } 2>&1
x=$(printf '%d' 1x 2>/dev/null 2>/dev/null); echo "[$x] $?"
x=$(command -v nope 2>/dev/null >/dev/null); echo "[$x] $?"

# A function comes first.
echo() { printf 'function %s\n' "$*"; }
x=$(echo a); command echo "[$x]"
unset -f echo

# Errors in the subshell don't end the shell.
set -e
x=$(printf '%d' 1x 2>/dev/null) || echo "failed $?"
x=$(printf '%d' 1x 2>/dev/null)
echo not reached
