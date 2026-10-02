# Function calls nest at most 1000 deep, as in Debian's dash (its patch 0009):
# the next call is an error, which exits a non-interactive shell with status 2.
# A debug build of luish needs more than the usual 8 MB of stack for 1000
# calls, so the shells under test get more.
ulimit -s 65536 2>/dev/null
# The shell's name starts its messages; dash also names eval there, luish doesn't
# (and adds the failing line and the call stack, left out here).
t() {
    { $SH -c "$1" 2>&1; echo "status $?"; } | sed '/^  /d; s/^[^:]*: //; s/eval: //'
}
echo '--- endless recursion'
t 'n=0; trap "echo depth \$n" EXIT; f() { n=$((n + 1)); f; }; f; echo not reached'
echo '--- in a subshell, only the subshell exits'
t 'f() { f; }; (f); echo "after $?"'
echo '--- 1000 nested calls are allowed, and the count goes down as they return'
t 'f() { [ "$1" -lt 1000 ] || return 7; f $(($1 + 1)); }; f 1; echo $?; f 1; echo $?'
echo '--- a function called through eval counts too'
t 'n=0; trap "echo depth \$n" EXIT; f() { n=$((n + 1)); eval f; }; f'
