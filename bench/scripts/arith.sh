# Arithmetic-heavy work with no external commands: a Mandelbrot set in fixed
# point, a sieve of Eratosthenes over eval'd pseudo-arrays, and Collatz
# sequence lengths. Exercises $((...)), `[`, `case`, while loops, eval, and a
# variable table with thousands of entries.
#
# All values stay below 2^31, so shells with 32-bit arithmetic (mksh) agree.

scale=${1:-1}

# Mandelbrot in 12-bit fixed point (4096 = 1.0).
mandel() {
	w=$1 h=$2 maxit=$3
	total=0
	y=0
	while [ "$y" -lt "$h" ]; do
		ci=$(( y * 8192 / h - 4096 ))
		line=
		x=0
		while [ "$x" -lt "$w" ]; do
			cr=$(( x * 12288 / w - 10240 ))
			zr=0 zi=0 i=0
			while [ "$i" -lt "$maxit" ]; do
				zr2=$(( zr * zr / 4096 ))
				zi2=$(( zi * zi / 4096 ))
				[ $(( zr2 + zi2 )) -gt 16384 ] && break
				zi=$(( 2 * zr * zi / 4096 + ci ))
				zr=$(( zr2 - zi2 + cr ))
				i=$(( i + 1 ))
			done
			total=$(( total + i ))
			case $i in
			"$maxit") c='#' ;;
			[0-2]) c=' ' ;;
			[3-5]) c='.' ;;
			[6-9]) c=':' ;;
			1[0-9]) c='+' ;;
			*) c='*' ;;
			esac
			line=$line$c
			x=$(( x + 1 ))
		done
		printf '%s\n' "$line"
		y=$(( y + 1 ))
	done
	printf 'mandelbrot iterations: %d\n' "$total"
}

# Sieve of Eratosthenes, one variable per number.
sieve() {
	n=$1
	i=2
	while [ "$i" -le "$n" ]; do
		eval "p_$i=1"
		i=$(( i + 1 ))
	done
	i=2
	while [ $(( i * i )) -le "$n" ]; do
		eval "v=\$p_$i"
		if [ "$v" = 1 ]; then
			j=$(( i * i ))
			while [ "$j" -le "$n" ]; do
				eval "p_$j=0"
				: $(( j += i ))
			done
		fi
		: $(( i += 1 ))
	done
	count=0 sum=0 last=0
	i=2
	while [ "$i" -le "$n" ]; do
		eval "v=\$p_$i"
		if [ "$v" = 1 ]; then
			count=$(( count + 1 ))
			sum=$(( (sum + i) % 1000000007 ))
			last=$i
		fi
		unset "p_$i"
		i=$(( i + 1 ))
	done
	printf 'primes <= %d: %d (sum mod p %d, largest %d)\n' "$n" "$count" "$sum" "$last"
}

# Longest Collatz sequence below a bound.
collatz() {
	limit=$1
	best=0 bestn=0
	n=1
	while [ "$n" -lt "$limit" ]; do
		k=$n steps=0
		while [ "$k" -ne 1 ]; do
			case $(( k % 2 )) in
			0) k=$(( k / 2 )) ;;
			*) k=$(( 3 * k + 1 )) ;;
			esac
			steps=$(( steps + 1 ))
		done
		if [ "$steps" -gt "$best" ]; then
			best=$steps bestn=$n
		fi
		n=$(( n + 1 ))
	done
	printf 'longest collatz below %d: %d (%d steps)\n' "$limit" "$bestn" "$best"
}

mandel 60 $(( 20 * scale )) 24
sieve $(( 6000 * scale ))
collatz $(( 1500 * scale ))
