# bash's `wait -n`, `-p` and `-f`, which dash and zsh lack. Jobs block on
# a fifo until the script lets them finish, so the order is deterministic.
mkfifo f1 f2
# Waits until a process has finished (is a zombie), without reaping it.
finished() {
    until read -r _ _ state _ </proc/$1/stat && [ "$state" = Z ]; do sleep 0.01; done
}

# `-n` returns the status of the first job to finish, `-p` names it, and
# with no job left the status is 127 and the variable is unset.
(read x <f1; exit 3) & a=$!
(read x <f2; exit 4) & b=$!
: >f2
wait -n -p v; echo "first: $? $([ "$v" = $b ] && echo b)"
: >f1
wait -n -p v; echo "second: $? $([ "$v" = $a ] && echo a)"
v=x; wait -n -p v; echo "none: $? ${v-unset}"

# Of jobs that have already finished, the one with the lowest number,
# whatever the order of the operands.
(exit 5) & a=$!
(exit 6) & b=$!
finished $a; finished $b
wait -n -p v $b $a; echo "lowest: $? $([ "$v" = $a ] && echo a)"
wait -n; echo "left: $?"
wait -n; echo "all waited for: $?"

# With operands, only those jobs count; an unknown pid is skipped.
(exit 8) & b=$!
finished $b
(read x <f1; exit 7) & a=$!
{ : >f1; } &
wait -n 1 $a; echo "named: $?"
wait -n; echo "then: $?"
wait -n; echo "releaser: $?"
wait -n 1; echo "not a child: $?"

# `-p` names the last process of a pipeline.
: | (exit 6) &
wait -n -p v; echo "pipeline: $? $([ "$v" = $! ] && echo last)"
set -o pipefail
(exit 3) | true &
wait -n; echo "pipefail: $?"
set +o pipefail

# Without `-n`, `-p` names the job of the last operand, if it is a child.
(exit 2) & a=$!
(exit 3) & b=$!
wait -p v $a $b; echo "last: $? $([ "$v" = $b ] && echo b)"
v=x; wait -p v $a 1; echo "not a child: $? ${v-unset}"
v=x; wait -p v; echo "no operand: $? ${v-unset}"
(exit 4) & wait -p v -- $!; echo "after --: $? $([ "$v" = $! ] && echo it)"
wait -npv; echo "joined: $? ${v-unset}"

# Without job control no job stops, so `-f` changes nothing.
(exit 9) & wait -f $!; echo "-f: $?"
sleep 0 & wait -f; echo "-f all: $?"

# Errors.
wait -p 1x; echo "bad name: $?"
wait -p; echo "no name: $?"
wait -x; echo "bad option: $?"
(readonly r; wait -p r; echo "not reached"); echo "read only: $?"
