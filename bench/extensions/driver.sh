# driver.sh IMPL TASK [SCALE]: runs a benchmark task with the commands of
# IMPL: `sh` (the functions of tasks.lsh) or `rhai` (the built-ins of
# ext.rhai). The task `load` only loads them. `wordfreq` reads $WORDS
# (made by run.sh with gen-words.awk).
impl=$1 task=$2 scale=${3:-1}
case $0 in */*) dir=${0%/*} ;; *) dir=. ;; esac
case $impl in
sh) . "$dir/tasks.lsh" ;;
rhai) __luish_internal plugin load "$dir/ext.rhai" || exit 1 ;;
esac
case $task in
load) ;;
collatz) collatz_max $(( 12000 * scale )) ;;
wordfreq) wordfreq "$WORDS" ;;
urlencode)
	i=0 total=0
	while [ $i -lt $(( 8000 * scale )) ]; do
		urlencode "/search?q=shell scripting & rhai #$i&lang=en-GB (fast!)"
		total=$(( total + ${#REPLY} ))
		i=$(( i + 1 ))
	done
	printf '%s\n%d\n' "$REPLY" $total
	;;
calls)
	i=0 acc=0
	while [ $i -lt $(( 300000 * scale )) ]; do
		add $acc $i
		acc=$REPLY
		i=$(( i + 1 ))
	done
	echo $acc
	;;
*) echo "driver.sh: unknown task: $task" >&2; exit 2 ;;
esac
