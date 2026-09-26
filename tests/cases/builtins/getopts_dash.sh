# getopts follows dash: OPTIND points past the current argument even when
# it has more option letters, OPTARG is kept at the end, and assigning
# OPTIND, set --, shift and function calls restart it.
show() { echo "st=$1 OPTIND=$OPTIND opt=$opt OPTARG=${OPTARG-unset}"; }
all() {
  spec=$1; shift
  while :; do
    getopts "$spec" opt "$@" 2>&1; st=$?
    show $st
    [ $st = 0 ] || break
  done
  OPTIND=1
}
all ab -ab
all c: -c10
all ab:c: -ab hi -c hello
all abc: -abc10
all ab -a -- -b
all ab -a - -b
all ab -a x -b
all a: -a
all :a: -a
all :a -b
all a -b -a
all ::a -:
all 'a:b' -a -b
set -u
getopts ab opt -a; show $?
set +u
unset OPTARG
getopts a opt -a -a; show $?
getopts a opt -a -a; show $?
getopts a opt -a; show $?
OPTIND=1
set -- -h -c foo x y z
while getopts "hc:" opt; do echo "- $opt"; done; echo OPTIND=$OPTIND
set --
while getopts "hc:" opt; do echo '-'; done; echo OPTIND=$OPTIND
set -- -a
while getopts "ab:" opt; do echo "$opt"; done
set -- -c -d -e E
while getopts "cde:" opt; do echo "$opt $OPTARG"; done
set -- -x -y
getopts xy opt; show $?
shift
getopts xy opt; show $?
set -- -x -y
getopts xy opt; show $?
f() { getopts pq opt; show $?; getopts pq opt; show $?; }
f -pq
getopts xy opt; show $?
getopts 'hc:' opt- -h; echo "status=$?"
getopts 2>/dev/null; echo "status=$?"
# A bad variable name is reported after OPTIND and OPTARG are set.
OPTIND=1
set -- -c foo -h
getopts 'hc:' opt- 2>/dev/null
echo "status=$? opt=$opt OPTARG=$OPTARG OPTIND=$OPTIND"
# OPTIND must be a number (dash's getoptsreset).
for v in -1 '' 0 abc 5 ' 3' '+4' 2147483648; do
  $SH -c "OPTIND='$v'; echo \"ok \$OPTIND\"" 2>/dev/null; echo "status $?"
done
$SH -c 'unset OPTIND; echo notreached' 2>/dev/null; echo "status $?"
for v in ' 3' '+3' '3 ' '-0' '03' '0x3' '-1' 2147483648; do
  $SH -c "exit '$v'" 2>/dev/null; echo "exit [$v] $?"
done
