# Recursion that doesn't go through functions, and deeply nested commands,
# are errors when the stack runs short, rather than crashing the shell as they
# crash dash. How deep they get depends on the build, so the tests don't show it
# (nor how often the call stack of the error repeats a line).
t() {
    { $SH "$@" 2>&1; echo "status $?"; } | sed 's/^[^:]*: //; s/ ([0-9]* times)$/ (N times)/'
}
nest() { # open middle close: 100000 levels of nesting
    awk -v o="$1" -v m="$2" -v c="$3" 'BEGIN {
        for (i = 0; i < 100000; i++) printf "%s", o; printf "%s", m
        for (i = 0; i < 100000; i++) printf "%s", c; print "" }' > nested.sh
}
echo '. ./self.sh' > self.sh
# (ulimit -s lowers the hard limit too, so each setting is in a subshell.)
(
    ulimit -s 8192 2>/dev/null
    echo '--- eval'
    t -c 'x='\''eval "$x"'\''; eval "$x"; echo not reached'
    echo '--- .'
    t -c '. ./self.sh; echo not reached'
    echo '--- a trap'
    t -c 'trap "kill -USR1 \$\$" USR1; kill -USR1 $$; echo not reached'
    echo '--- nested subshells'
    nest '(' true ')'; t nested.sh
    echo '--- nested command substitutions'
    nest '$(' true ')'; t nested.sh
    echo '--- nested parameter expansions'
    nest 'echo "${x:-' a '}"'; t nested.sh
    echo '--- nested arithmetic'
    nest 'echo $((' 1 '))'; t nested.sh
    awk 'BEGIN { printf "echo $(("; for (i = 0; i < 100000; i++) printf "-"; print "1))" }' > nested.sh; t nested.sh
)
(
    ulimit -s 65536 2>/dev/null
    echo '--- an interactive shell goes back to the prompt, and can still call functions'
    printf '%s\n' 'f() { f; }' 'f' 'x='\''eval "$x"'\''; eval "$x"' \
        'g() { [ "$1" -lt 900 ] || return 3; g $(($1 + 1)); }' 'g 1; echo $?' |
        PS1= PS2= $SH -i 2>&1 | sed 's/^[^:]*: //' | grep -v 'job control'
)
