#!/bin/sh
# Compares an extension's built-ins (ext.rhai) with the same commands as
# luish shell functions (tasks.lsh). See README.md.
#
# Usage: bench/extensions/run.sh [-n SCALE] [-r RUNS] [-s SHELL] [TASK]...
#
#   -n SCALE   work size (default 1: about a second for the shell functions)
#   -r RUNS    runs per implementation and task (default 5)
#   -s SHELL   the luish to use (default target/release/luish)
#   TASK       load, collatz, wordfreq, urlencode or calls (default: all)

set -u
dir=$(cd "$(dirname "$0")" && pwd) || exit 1
root=${dir%/bench/extensions}
scale=1 runs=5 luish=$root/target/release/luish
while getopts n:r:s: opt; do
	case $opt in
	n) scale=$OPTARG ;;
	r) runs=$OPTARG ;;
	s) luish=$OPTARG ;;
	*) exit 2 ;;
	esac
done
shift $(( OPTIND - 1 ))
[ $# -gt 0 ] || set -- load collatz wordfreq urlencode calls
[ -x "$luish" ] || { echo "run.sh: $luish: not found (pixi run release)" >&2; exit 1; }

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
awk -v lines=$(( 20000 * scale )) -f "$dir/gen-words.awk" > "$tmp/words.txt"

now() { date +%s%N; }
# Runs the task $2 with the implementation $1 $runs times; sets `ms` to the
# fastest time and `mean` to the mean, in milliseconds.
time_task() {
	ms= total=0 i=0
	while [ $i -lt $runs ]; do
		t0=$(now)
		env -i PATH="$PATH" LC_ALL=C WORDS="$tmp/words.txt" "$luish" "$dir/driver.sh" $1 $2 $scale > /dev/null
		t=$(( ($(now) - t0) / 1000 ))
		total=$(( total + t ))
		if [ -z "$ms" ] || [ $t -lt $ms ]; then ms=$t; fi
		i=$(( i + 1 ))
	done
	mean=$(( total / runs ))
}
fmt() { printf '%d.%d' $(( $1 / 1000 )) $(( $1 % 1000 / 100 )); }

status=0
printf '| Task | Shell functions (ms) | Rhai built-ins (ms) | Shell / Rhai |\n|---|--:|--:|--:|\n'
for task; do
	env -i PATH="$PATH" LC_ALL=C WORDS="$tmp/words.txt" "$luish" "$dir/driver.sh" sh $task $scale > "$tmp/sh.out"
	env -i PATH="$PATH" LC_ALL=C WORDS="$tmp/words.txt" "$luish" "$dir/driver.sh" rhai $task $scale > "$tmp/rhai.out"
	if ! cmp -s "$tmp/sh.out" "$tmp/rhai.out"; then
		echo "run.sh: $task: the outputs differ" >&2
		diff "$tmp/sh.out" "$tmp/rhai.out" | head -5 >&2
		status=1
		continue
	fi
	time_task sh $task; sh_ms=$ms sh_mean=$mean
	time_task rhai $task; rhai_ms=$ms rhai_mean=$mean
	ratio=$(( sh_ms * 100 / rhai_ms ))
	printf '| %s | %s (mean %s) | %s (mean %s) | %d.%02d |\n' $task \
		"$(fmt $sh_ms)" "$(fmt $sh_mean)" "$(fmt $rhai_ms)" "$(fmt $rhai_mean)" $(( ratio / 100 )) $(( ratio % 100 ))
done
exit $status
