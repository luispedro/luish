# `seq` with small integers runs in the shell when it is the system's
# (`exec/utility.rs`), and `$(seq ...)` without a subshell; anything else
# runs the utility. Output and status must be the utility's.

t() {
    x=$(seq "$@" 2>/dev/null); s=$?
    printf '%s => [%s] %s | ' "$*" "$(echo $x)" "$s"
    seq "$@" > out 2>/dev/null; echo "$? $(echo $(cat out))"
}
t 3
t 0
t -2
t 2 4
t 4 2
t 3 3
t 01 03
t 5 -1 1
t 5 -2 1
t 1 2 6
t 1 -1 3
t -3 -1
t -3 2 3
t 999999999999998 999999999999999
t -999999999999999 -999999999999998
# The utility's.
t 1 0 3
t -0 2
t 0 -0 3
t +1 3
t -- 1 3
t 1.5 3
t 1 1e1
t 1 0x3
t -s , 1 3
t -w 8 10
t 9223372036854775806 9223372036854775807
t 1 2 3 4
t
t a

for i in $(seq 3); do echo "i=$i"; done
for i in $(seq 10 -5 0); do echo "i=$i"; done
x=$(seq 1 100000); echo "${#x}"
x=$(seq 1 100001); echo "${#x}"
seq 2 | cat
false; x=$(seq 5 1); echo "[$x] $?"
