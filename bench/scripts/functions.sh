# Function-heavy code, like a script built on a shell library: recursion,
# `local`, argument forwarding through wrappers with "$@", `shift` and
# `set --`, getopts in a function called many times, return statuses in
# conditions, and a stack and queue kept in positional parameters and
# strings. No external commands.
#
# `local` is not POSIX but dash, bash, zsh, mksh and busybox all have it.

scale=${1:-1}

# Towers of Hanoi: counts moves and checks a sample of them.
hanoi() {
	local n=$1 from=$2 to=$3 via=$4
	[ "$n" -eq 0 ] && return 0
	hanoi $(( n - 1 )) "$from" "$via" "$to"
	moves=$(( moves + 1 ))
	case $(( moves % 4096 )) in
	0) printf 'move %d: disk %d %s -> %s\n' "$moves" "$n" "$from" "$to" ;;
	esac
	hanoi $(( n - 1 )) "$via" "$to" "$from"
}

# Naive Fibonacci, result in $REPLY.
fib() {
	if [ "$1" -lt 2 ]; then
		REPLY=$1
		return
	fi
	local a
	fib $(( $1 - 1 ))
	a=$REPLY
	fib $(( $1 - 2 ))
	REPLY=$(( a + REPLY ))
}

# Ackermann, small arguments only.
ack() {
	if [ "$1" -eq 0 ]; then
		REPLY=$(( $2 + 1 ))
	elif [ "$2" -eq 0 ]; then
		ack $(( $1 - 1 )) 1
	else
		ack "$1" $(( $2 - 1 ))
		ack $(( $1 - 1 )) "$REPLY"
	fi
}

# Logging through layers of wrappers, as libraries do.
log_level=2
_log() {
	local level=$1 msg
	shift
	[ "$level" -le "$log_level" ] || return 1
	msg="$*"
	logged=$(( logged + 1 ))
	logbytes=$(( logbytes + ${#msg} ))
}
log_info() { _log 2 "$@"; }
log_debug() { _log 3 "$@"; }
log_warn() { _log 1 "warning:" "$@"; }

# A command-line parser, called once per simulated invocation.
parse_args() {
	local OPTIND=1 opt
	verbose=0 output= jobs=1 force=false
	while getopts vo:j:fh opt; do
		case $opt in
		v) verbose=$(( verbose + 1 )) ;;
		o) output=$OPTARG ;;
		j) jobs=$OPTARG ;;
		f) force=true ;;
		h) return 2 ;;
		*) return 1 ;;
		esac
	done
	shift $(( OPTIND - 1 ))
	nargs=$#
}

is_even() { [ $(( $1 % 2 )) -eq 0 ]; }
is_div3() { return $(( $1 % 3 != 0 )); }
classify() {
	if is_even "$1" && is_div3 "$1"; then
		REPLY=six
	elif is_even "$1"; then
		REPLY=even
	elif ! is_div3 "$1"; then
		REPLY=odd
	else
		REPLY=three
	fi
}

# A stack in the positional parameters: pushes numbers, popping and
# summing half of them whenever it grows past 40.
stack_demo() {
	local n=$1 i=0 sum=0
	set --
	while [ "$i" -lt "$n" ]; do
		set -- "$i" "$@"
		i=$(( i + 1 ))
		if [ "$#" -gt 40 ]; then
			while [ "$#" -gt 20 ]; do
				sum=$(( sum + $1 ))
				shift
			done
		fi
	done
	printf 'stack: %d left, popped sum %d\n' "$#" "$sum"
}

# A queue in a string: breadth-first numbering of a binary tree.
queue_demo() {
	local limit=$1 queue=1 visited=0 sum=0 node
	while [ -n "$queue" ]; do
		node=${queue%% *}
		case $queue in
		*' '*) queue=${queue#* } ;;
		*) queue= ;;
		esac
		visited=$(( visited + 1 ))
		sum=$(( sum + node ))
		[ $(( node * 2 )) -le "$limit" ] && queue="${queue:+$queue }$(( node * 2 ))"
		[ $(( node * 2 + 1 )) -le "$limit" ] && queue="${queue:+$queue }$(( node * 2 + 1 ))"
	done
	printf 'queue: visited %d, sum %d\n' "$visited" "$sum"
}

# The recursive parts have fixed sizes, repeated $scale times, so that the
# running time grows linearly with the scale.
rep=0
while [ "$rep" -lt "$scale" ]; do
	moves=0
	hanoi 13 A C B
	printf 'hanoi moves: %d\n' "$moves"
	fib 17
	printf 'fib: %d\n' "$REPLY"
	ack 2 80
	printf 'ack: %d\n' "$REPLY"
	rep=$(( rep + 1 ))
done

logged=0 logbytes=0 i=0
six=0 even=0 odd=0 three=0 bad=0
while [ "$i" -lt $(( 3000 * scale )) ]; do
	log_info "iteration" "$i" "of" "the main loop"
	log_debug "not shown" "$i"
	log_warn "something odd with" "$i"
	case $(( i % 4 )) in
	0) parse_args -v -o "out$i" -j 4 file1 file2 ;;
	1) parse_args -vv -f -- "arg $i" ;;
	2) parse_args -j"$i" a b c d ;;
	3) parse_args -x 2>/dev/null || bad=$(( bad + 1 )) ;;
	esac
	classify "$i"
	eval "$REPLY=\$(( $REPLY + 1 ))"
	i=$(( i + 1 ))
done
printf 'logged %d (%d bytes), last args: verbose=%d output=%s jobs=%s force=%s nargs=%d, bad %d\n' \
	"$logged" "$logbytes" "$verbose" "$output" "$jobs" "$force" "$nargs" "$bad"
printf 'classes: six %d, even %d, odd %d, three %d\n' "$six" "$even" "$odd" "$three"

stack_demo $(( 3000 * scale ))
queue_demo $(( 1000 * scale ))
