# `expr` with integers runs in the shell when it is the system's
# (`exec/utility.rs`), and `$(expr ...)` without a subshell; anything else
# runs the utility. Output and status must be the utility's.

t() {
    x=$(expr "$@"); s=$?
    printf '%s => [%s] %s | ' "$*" "$x" "$s"
    expr "$@"; echo "$?"
}
t 1 + 2
t 007 + 1
t 007
t '(' 007 ')'
t -5 + 1
t 1 + -5
t 1 - 1
t 0 + 0
t 00 + 0
t -0 + 0
t 7 / 2
t -7 / 2
t -7 % 2
t 7 % -2
t 2 '*' 3 + 4
t 2 + 3 '*' 4
t '(' 2 + 3 ')' '*' 4
t '(' '(' 1 ')' ')' + '(' 2 - 5 ')'
t 10 '>' 9
t 10 '<' 9
t 9 '>=' 9
t 9 '<=' 8
t 9 '!=' 9
t 9 = 09
t 1 '<' 2 '<' 3
t 1 + 2 = 3
t 3 = 1 + 2 '*' 1
t 1 / 0
t 1 % 0
t 9223372036854775807 + 1
t 9223372036854775807 + 0
t -9223372036854775807 - 1
t 99999999999999999999 + 1
t 3037000500 '*' 3037000500
t +5 + 1
t 5 + +1
t -- 5 + 1
t a + 1
t a = a
t a = b
t abc : 'a\(.\)'
t abc : 'a.*'
t 2 '|' 3
t 0 '&' 3
t length abc
t 1 +
t '(' 1 + 2
t 1 + 2 ')'
t 3 - - 1
t ' 1' + 1
t 1 + ''
t

# The status counts under set -e, as for any command substitution.
i=2
while i=$(expr $i - 1); do echo "i=$i"; done
echo "end i=$i"
set -e
n=$(expr 5 - 5) || echo "zero $n $?"
expr 1 '*' 0 >/dev/null || echo "zero again"
n=$(expr 2 - 5)
echo "n=$n"
n=$(expr 3 - 3)
echo not reached
