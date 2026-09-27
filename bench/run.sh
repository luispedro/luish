#!/bin/sh
# Runs the benchmark scripts in bench/scripts under several shells.
#
# Usage: bench/run.sh [-c] [-n SCALE] [-r RUNS] [-o FILE] [-s NAME=COMMAND]... [BENCHMARK]...
#
#   -c              only check that every shell prints the same output as the first
#   -n SCALE        work size passed to each script (default 1)
#   -r RUNS         runs per shell and benchmark (default: hyperfine's choice, or 5)
#   -o FILE         also write the summary table (Markdown) to FILE
#   -s NAME=CMD     a shell to compare (repeatable; replaces the default list).
#                   CMD may have arguments, e.g. -s 'zsh=zsh --emulate sh'.
#   BENCHMARK       script names without .sh (default: all of bench/scripts)
#
# The default shells are dash (the reference), luish (target/release/luish,
# built with `pixi run release`), bash --posix, zsh --emulate sh, and busybox
# sh, whichever are installed. Each script runs with a cleared environment
# (PATH, HOME, LC_ALL=C and SH, which is set to the shell's command), and its
# output is first compared with the reference shell's: timings of a shell
# whose output differs are flagged, as they are not comparable. Timing uses
# hyperfine if it is installed, otherwise a simple loop.

set -u

bench_dir=$(cd "$(dirname "$0")" && pwd) || exit 1
root=${bench_dir%/bench}

check_only=false scale=1 runs= outfile= shells= nshells=0
while getopts cn:r:o:s: opt; do
	case $opt in
	c) check_only=true ;;
	n) scale=$OPTARG ;;
	r) runs=$OPTARG ;;
	o) outfile=$OPTARG ;;
	s)
		case $OPTARG in
		?*=?*) ;;
		*) printf 'run.sh: -s wants NAME=COMMAND\n' >&2; exit 2 ;;
		esac
		_cmd=${OPTARG#*=}
		# The scripts change directory, so make a relative path absolute.
		case $_cmd in
		/*) ;;
		*/*) _cmd=$PWD/$_cmd ;;
		esac
		nshells=$(( nshells + 1 ))
		eval "shell_name_$nshells=\${OPTARG%%=*} shell_cmd_$nshells=\$_cmd"
		;;
	*) exit 2 ;;
	esac
done
shift $(( OPTIND - 1 ))

add_shell() {
	nshells=$(( nshells + 1 ))
	eval "shell_name_$nshells=\$1 shell_cmd_$nshells=\$2"
}

if [ "$nshells" -eq 0 ]; then
	command -v dash >/dev/null && add_shell dash dash
	if [ -x "$root/target/release/luish" ]; then
		add_shell luish "$root/target/release/luish"
	else
		printf 'run.sh: %s not built (pixi run release); skipping luish\n' "$root/target/release/luish" >&2
	fi
	command -v bash >/dev/null && add_shell bash 'bash --posix'
	command -v zsh >/dev/null && add_shell zsh 'zsh --emulate sh'
	command -v busybox >/dev/null && add_shell busybox 'busybox sh'
fi
[ "$nshells" -gt 0 ] || { printf 'run.sh: no shells to compare\n' >&2; exit 2; }

if [ "$#" -eq 0 ]; then
	for f in "$bench_dir"/scripts/*.sh; do
		f=${f##*/}
		set -- "$@" "${f%.sh}"
	done
fi
for b in "$@"; do
	[ -f "$bench_dir/scripts/$b.sh" ] || { printf 'run.sh: no benchmark %s\n' "$b" >&2; exit 2; }
done

tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/home"

# Prints the command line that runs benchmark $1 under shell number $2,
# quoted for hyperfine --shell=none (and for eval).
command_for() {
	eval "_cmd=\$shell_cmd_$2"
	printf "env -i PATH='%s' HOME='%s' LC_ALL=C 'SH=%s' %s '%s' %s" \
		"$PATH" "$tmp/home" "$_cmd" "$_cmd" "$bench_dir/scripts/$1.sh" "$scale"
}

# Checks the output of every shell against the first one's; records the
# shells that differ in differs_BENCH_N.
status=0
printf 'Checking outputs (scale %s):\n' "$scale"
for b in "$@"; do
	printf '  %-12s' "$b"
	i=1
	while [ "$i" -le "$nshells" ]; do
		eval "name=\$shell_name_$i"
		eval "$(command_for "$b" "$i")" > "$tmp/out.$i" 2> "$tmp/err.$i"
		printf 'exit status %d\n' "$?" >> "$tmp/out.$i"
		differs=
		if [ "$i" -gt 1 ] && ! cmp -s "$tmp/out.1" "$tmp/out.$i"; then
			differs=1
			eval "differs_${b}_$i=1"
			printf ' %s:DIFFERS' "$name"
			status=1
		else
			printf ' %s:ok' "$name"
		fi
		if [ -s "$tmp/err.$i" ]; then
			printf '(stderr: %s)' "$(head -1 "$tmp/err.$i" | cut -c1-60)"
		fi
		if [ -n "$differs" ] && $check_only; then
			printf '\n'
			diff "$tmp/out.1" "$tmp/out.$i" | sed 's/^/      /' | head -20
		fi
		i=$(( i + 1 ))
	done
	printf '\n'
done
$check_only && exit "$status"

# Times benchmark $1 on every shell, writing "name mean stddev" lines (in
# seconds) to $tmp/$1.times.
time_benchmark() {
	if command -v hyperfine >/dev/null; then
		set -- "$1" hyperfine -N -i --warmup 1 --style basic --export-csv "$tmp/$1.csv"
		if [ -n "$runs" ]; then
			set -- "$@" --runs "$runs"
		else
			set -- "$@" --min-runs 5
		fi
		i=1
		while [ "$i" -le "$nshells" ]; do
			eval "name=\$shell_name_$i"
			set -- "$@" -n "$name" "$(command_for "$1" "$i")"
			i=$(( i + 1 ))
		done
		_b=$1
		shift
		"$@" || return 1
		# command,mean,stddev,median,user,system,min,max
		sed 1d "$tmp/$_b.csv" | awk -F, '{ print $1, $2, $3 }' > "$tmp/$_b.times"
	else
		: > "$tmp/$1.times"
		i=1
		while [ "$i" -le "$nshells" ]; do
			eval "name=\$shell_name_$i"
			cmd=$(command_for "$1" "$i")
			n=0
			: > "$tmp/samples"
			while [ "$n" -lt "${runs:-5}" ]; do
				t0=$(date +%s%N)
				eval "$cmd" > /dev/null 2>&1
				t1=$(date +%s%N)
				printf '%s\n' $(( t1 - t0 )) >> "$tmp/samples"
				n=$(( n + 1 ))
			done
			awk -v name="$name" '{ s += $1; ss += $1 * $1; n++ }
				END { m = s / n; v = ss / n - m * m; if (v < 0) v = 0
				      printf "%s %.6f %.6f\n", name, m / 1e9, sqrt(v) / 1e9 }' \
				"$tmp/samples" >> "$tmp/$1.times"
			printf '  %-10s %s\n' "$name" "$(tail -1 "$tmp/$1.times" | awk '{ printf "%.3f s", $2 }')"
			i=$(( i + 1 ))
		done
	fi
}

for b in "$@"; do
	printf '\n== %s ==\n' "$b"
	time_benchmark "$b" || exit 1
done

# Summary: mean time per benchmark and shell, and the ratio to the first
# shell.
{
	printf '\nMean time in seconds (ratio to %s), scale %s:\n\n' "$shell_name_1" "$scale"
	printf '| Benchmark |'
	i=1
	while [ "$i" -le "$nshells" ]; do
		eval "printf ' %s |' \"\$shell_name_$i\""
		i=$(( i + 1 ))
	done
	printf '\n|---|'
	i=1
	while [ "$i" -le "$nshells" ]; do
		printf '%s' '---|'
		i=$(( i + 1 ))
	done
	printf '\n'
	for b in "$@"; do
		printf '| %s |' "$b"
		ref=$(awk 'NR == 1 { print $2 }' "$tmp/$b.times")
		i=1
		while [ "$i" -le "$nshells" ]; do
			eval "name=\$shell_name_$i"
			eval "flag=\${differs_${b}_$i-}"
			awk -v name="$name" -v ref="$ref" -v flag="${flag:+ (output differs)}" \
				'$1 == name { printf " %.3f (%.2f)%s |", $2, $2 / ref, flag }' "$tmp/$b.times"
			i=$(( i + 1 ))
		done
		printf '\n'
	done
} > "$tmp/summary"
cat "$tmp/summary"
if [ -n "$outfile" ]; then
	sed 1d "$tmp/summary" > "$outfile"
fi
exit "$status"
