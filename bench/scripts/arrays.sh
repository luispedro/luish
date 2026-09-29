# Arrays and the other extensions that zsh and bash scripts use: indexed
# arrays (a stack, a queue, sorting, a sieve, flattened matrices, sliding
# windows), associative arrays (counting and grouping), `a+=(x)`,
# `$(( a[i] ))`, `${x//pat/rep}`, `${x:offset:length}`, `[[ ... ]]` and
# `local -a`. No external commands.
#
# Not POSIX: dash and BusyBox can't run it, and mksh's arrays differ.
# skip: dash busybox mksh
#
# It sticks to what luish, `bash --posix` and `zsh --emulate sh` agree on:
# no `(( ))` or `**`, no holes in arrays (bash keeps them, zsh fills them),
# no `${!h[@]}` (zsh has `${(k)h}`) and so no reliance on the order of an
# associative array's keys (a separate array keeps it), no spaces in the
# subscript of an assignment (`a[i+1]=x`; zsh splits `a[i + 1]=x`), no
# names as offsets (`${a[@]:$i:5}`; zsh and luish take `:i` for a
# modifier), keys
# with spaces only through variables, and only numbers in `$(( a[i] ))`.

scale=${1:-1}

# A linear congruential generator; the next number (0 to 32767) is in $REPLY.
seed=12345
rand() {
	seed=$(( (seed * 1103515245 + 12345) % 2147483648 ))
	REPLY=$(( seed / 65536 % 32768 ))
}

# Fills the array `nums` with $1 random numbers below $2.
fill() {
	local i=0
	nums=()
	while [ $i -lt "$1" ]; do
		rand
		nums+=($(( REPLY % $2 )))
		i=$(( i + 1 ))
	done
}

# Sorts `nums` in place: quicksort (Lomuto partition) with an explicit
# stack of ranges, and insertion sort for short ranges.
sort_nums() {
	local -a stack
	local sp lo hi i j p t pivot
	stack=(0 $(( ${#nums[@]} - 1 )))
	sp=2
	while [ $sp -gt 0 ]; do
		sp=$(( sp - 2 ))
		lo=${stack[sp]} hi=${stack[sp + 1]}
		if [ $(( hi - lo )) -lt 8 ]; then
			i=$(( lo + 1 ))
			while [ $i -le $hi ]; do
				t=${nums[i]}
				j=$(( i - 1 ))
				while [ $j -ge $lo ] && [ "${nums[j]}" -gt "$t" ]; do
					nums[j+1]=${nums[j]}
					j=$(( j - 1 ))
				done
				nums[j+1]=$t
				i=$(( i + 1 ))
			done
			continue
		fi
		# The median of three as the pivot, moved to the end.
		i=$(( (lo + hi) / 2 ))
		if [ $(( nums[i] < nums[lo] )) = 1 ]; then
			t=${nums[i]} nums[i]=${nums[lo]} nums[lo]=$t
		fi
		if [ $(( nums[hi] < nums[lo] )) = 1 ]; then
			t=${nums[hi]} nums[hi]=${nums[lo]} nums[lo]=$t
		fi
		if [ $(( nums[i] < nums[hi] )) = 1 ]; then
			t=${nums[i]} nums[i]=${nums[hi]} nums[hi]=$t
		fi
		pivot=${nums[hi]}
		p=$lo
		j=$lo
		while [ $j -lt $hi ]; do
			if [ "${nums[j]}" -lt "$pivot" ]; then
				t=${nums[j]} nums[j]=${nums[p]} nums[p]=$t
				p=$(( p + 1 ))
			fi
			j=$(( j + 1 ))
		done
		t=${nums[p]} nums[p]=${nums[hi]} nums[hi]=$t
		stack[sp]=$lo stack[sp+1]=$(( p - 1 ))
		stack[sp+2]=$(( p + 1 )) stack[sp+3]=$hi
		sp=$(( sp + 4 ))
	done
}

# Checks that `nums` is sorted, and prints a summary of it.
check_sorted() {
	local i=1 n=${#nums[@]} sum=${nums[0]}
	while [ $i -lt $n ]; do
		if [ $(( nums[i - 1] > nums[i] )) = 1 ]; then
			printf 'not sorted at %d\n' $i
			return 1
		fi
		sum=$(( sum + nums[i] ))
		i=$(( i + 1 ))
	done
	printf 'sorted %d: min %d, median %d, max %d, sum %d\n' \
		$n "${nums[0]}" "${nums[n / 2]}" "${nums[n - 1]}" $sum
	printf 'first: %s\n' "${nums[*]:0:8}"
}

sorting() {
	local round=0
	while [ $round -lt "$scale" ]; do
		fill 1500 30000
		sort_nums
		check_sorted
		round=$(( round + 1 ))
	done
}

# The sieve of Eratosthenes in an array of flags.
sieve() {
	local n=$(( 12000 * scale )) i j f count=0 last=0
	local -a flags
	i=0
	while [ $i -le $n ]; do
		flags+=(1)
		i=$(( i + 1 ))
	done
	flags[0]=0 flags[1]=0
	i=2
	while [ $(( i * i )) -le $n ]; do
		if [ "${flags[i]}" = 1 ]; then
			j=$(( i * i ))
			while [ $j -le $n ]; do
				flags[j]=0
				j=$(( j + i ))
			done
		fi
		i=$(( i + 1 ))
	done
	i=0
	for f in "${flags[@]}"; do
		if [ "$f" = 1 ]; then
			count=$(( count + 1 ))
			last=$i
		fi
		i=$(( i + 1 ))
	done
	printf 'primes up to %d: %d, the largest %d\n' $n $count $last
}

# Words from a vocabulary, counted in an associative array, with the order
# of first appearance kept in `seen`.
vocabulary=(alpha bravo charlie delta echo foxtrot golf hotel india juliet
	kilo lima mike november oscar papa quebec romeo sierra tango uniform
	victor whiskey xray yankee zulu 'new york' 'los angeles' 'san jose')
typeset -A count
word_counts() {
	local n=$(( 6000 * scale )) i=0 w v=${#vocabulary[@]} k best best_n
	local -a seen
	while [ $i -lt $n ]; do
		rand
		# Skewed towards the start of the vocabulary.
		w=${vocabulary[REPLY % v * (REPLY / 7 % v) / v]}
		if [ -z "${count[$w]+set}" ]; then
			seen+=("$w")
		fi
		count[$w]=$(( ${count[$w]:-0} + 1 ))
		i=$(( i + 1 ))
	done
	printf 'distinct words: %d\n' ${#seen[@]}
	# The five most frequent, by selection; ties go to the first seen.
	for k in 1 2 3 4 5; do
		best= best_n=0
		for w in "${seen[@]}"; do
			if [ "${count[$w]}" -gt $best_n ]; then
				best=$w best_n=${count[$w]}
			fi
		done
		printf '  %d. %s (%d)\n' $k "$best" $best_n
		count[$best]=0
	done
}

# Records split into arrays with IFS, grouped by department in
# associative arrays, with string operations on the fields.
typeset -A total staff top top_pay
records() {
	local n=$(( 2500 * scale )) i=0 line name dept pay d initials
	local -a fields depts names
	names=(ada grace alan edsger barbara donald ken dennis margaret niklaus)
	depts=(eng ops sales research legal)
	while [ $i -lt $n ]; do
		rand
		name=${names[REPLY % 10]}_$(( REPLY % 97 ))
		dept=${depts[REPLY / 10 % 5]}
		pay=$(( 30000 + REPLY % 50000 ))
		line="$name,$dept,$pay"
		IFS=,
		fields=($line)
		unset IFS
		name=${fields[0]} dept=${fields[1]} pay=${fields[2]}
		if [ -z "${staff[$dept]+set}" ]; then
			total[$dept]=0 staff[$dept]=0 top_pay[$dept]=0
		fi
		total[$dept]=$(( ${total[$dept]} + pay ))
		staff[$dept]=$(( ${staff[$dept]} + 1 ))
		if [ "$pay" -gt "${top_pay[$dept]}" ]; then
			top_pay[$dept]=$pay
			# Capitalized, with ` #` before the number.
			initials=${name:0:1}
			case $initials in
			a) initials=A ;; b) initials=B ;; d) initials=D ;; e) initials=E ;;
			g) initials=G ;; k) initials=K ;; m) initials=M ;; n) initials=N ;;
			esac
			top[$dept]="$initials${name:1}"
			top[$dept]=${top[$dept]/_/ #}
		fi
		i=$(( i + 1 ))
	done
	for d in "${depts[@]}"; do
		printf '%-8s %4d staff, average %d, best paid %s (%d)\n' "$d" \
			"${staff[$d]}" $(( ${total[$d]} / ${staff[$d]} )) "${top[$d]}" "${top_pay[$d]}"
	done
}

# Matrix products with the matrices flattened into arrays.
matrices() {
	local n=18 round=0 i j k s trace
	local -a a b c
	while [ $round -lt "$scale" ]; do
		a=() b=()
		i=0
		while [ $i -lt $(( n * n )) ]; do
			rand
			a+=($(( REPLY % 19 - 9 )))
			b+=($(( REPLY / 19 % 19 - 9 )))
			i=$(( i + 1 ))
		done
		c=()
		i=0
		while [ $i -lt $n ]; do
			j=0
			while [ $j -lt $n ]; do
				s=0 k=0
				while [ $k -lt $n ]; do
					s=$(( s + a[i * n + k] * b[k * n + j] ))
					k=$(( k + 1 ))
				done
				c+=($s)
				j=$(( j + 1 ))
			done
			i=$(( i + 1 ))
		done
		trace=0 i=0
		while [ $i -lt $n ]; do
			trace=$(( trace + c[i * n + i] ))
			i=$(( i + 1 ))
		done
		printf 'product %d: trace %d, corners %d %d %d %d\n' $round $trace \
			"${c[0]}" "${c[n - 1]}" "${c[n * (n - 1)]}" "${c[n * n - 1]}"
		round=$(( round + 1 ))
	done
}

# Breadth-first search on a grid with walls, with the queue and the
# distances in arrays.
grid_bfs() {
	local w=$(( 40 * scale )) h=40 i cell x y next head=0 far=0 reached=0
	local -a wall dist queue
	i=0
	while [ $i -lt $(( w * h )) ]; do
		rand
		if [ $(( REPLY % 100 )) -lt 28 ]; then
			wall+=(1)
		else
			wall+=(0)
		fi
		dist+=(-1)
		i=$(( i + 1 ))
	done
	# The start and its neighbours are open.
	wall[0]=0 wall[1]=0 wall[w]=0
	dist[0]=0
	queue=(0)
	while [ $head -lt ${#queue[@]} ]; do
		cell=${queue[head]}
		head=$(( head + 1 ))
		x=$(( cell % w )) y=$(( cell / w ))
		for next in $(( x > 0 ? cell - 1 : -1 )) $(( x < w - 1 ? cell + 1 : -1 )) \
			$(( y > 0 ? cell - w : -1 )) $(( y < h - 1 ? cell + w : -1 )); do
			[ $next -lt 0 ] && continue
			[ "${wall[next]}" = 1 ] && continue
			[ "${dist[next]}" != -1 ] && continue
			dist[next]=$(( dist[cell] + 1 ))
			queue+=($next)
		done
	done
	for i in "${dist[@]}"; do
		if [ $i -ge 0 ]; then
			reached=$(( reached + 1 ))
			[ $i -gt $far ] && far=$i
		fi
	done
	printf 'grid %dx%d: %d cells reached, the farthest %d steps away\n' $w $h $reached $far
}

# Sliding windows over arrays with slices, and string building with
# pattern substitutions and [[ ]] on the elements. The arrays are of a fixed
# size, as bash copies its linked list up to a slice's offset.
windows() {
	local n=400 round=0 i best best_at s x
	local -a vals win tags
	while [ $round -lt "$scale" ]; do
		fill $n 1000
		vals=("${nums[@]}")
		i=0 best=-1 best_at=0
		while [ $(( i + 5 )) -le $n ]; do
			win=("${vals[@]:$i:5}")
			s=0
			for x in "${win[@]}"; do
				s=$(( s + x ))
			done
			if [ $s -gt $best ]; then
				best=$s best_at=$i
			fi
			i=$(( i + 1 ))
		done
		printf 'best window at %d: %s (sum %d)\n' $best_at "${vals[*]:$best_at:5}" $best
		tags=()
		for x in "${vals[@]:0:100}"; do
			if [[ $x == *7* ]]; then
				tags+=("seven:$x")
			elif [[ $x == ?? ]]; then
				tags+=("small:$x")
			elif [[ $x < 5 ]]; then
				tags+=("low:$x")
			else
				tags+=("other:$x")
			fi
		done
		s="${tags[*]}"
		x=${s//seven/}
		printf 'tags: %d, sevens %d\n' ${#tags[@]} $(( (${#s} - ${#x}) / 5 ))
		s=${s//other:/o}
		s=${s// /,}
		printf 'joined: %s...\n' "${s:0:60}"
		tags=("${tags[@]/#low:/L}")
		printf 'relabelled: %s\n' "${tags[*]:0:6}"
		round=$(( round + 1 ))
	done
}

sorting
sieve
word_counts
records
matrices
grid_bfs
windows
