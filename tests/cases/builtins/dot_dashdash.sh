# `.` has no options, as in dash: a first `--` is skipped, a later one is a
# file, and another word starting with `-` is an illegal option.
printf 'echo sourced\nv=set\n' > inc.sh
printf 'echo dash-x\n' > ./-x
. -- ./inc.sh; echo "$? $v"
PATH=$PWD:$PATH
. -- -x
(. -- --); echo "status $?"
. -x
echo not reached
