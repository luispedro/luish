# Text processing in the shell itself, the way many admin scripts do it:
# generate a web server access log and an INI file, then parse them with
# `while read` loops, field splitting, `case` and prefix/suffix removal,
# aggregating into eval'd variables. The only external commands are
# mktemp, rm and one `sort` for the report.

scale=${1:-1}
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
cd "$work" || exit 1

seed=42
# A small LCG (fits in 32-bit arithmetic); leaves the number in $seed.
rand() {
	seed=$(( (seed * 75 + 74) % 65537 ))
}

gen_log() {
	n=$1
	paths='/ /index.html /about /api/v1/users /api/v1/orders /api/v2/items /static/app.js /static/style.css /login /logout /search /admin/stats'
	statuses='200 200 200 200 200 200 304 301 404 404 403 500'
	i=0
	while [ "$i" -lt "$n" ]; do
		rand; a=$(( seed % 4 )); b=$(( seed % 8 ))
		rand; c=$(( seed % 256 ))
		rand; set -- $paths; shift $(( seed % $# )); path=$1
		rand; set -- $statuses; shift $(( seed % $# )); status=$1
		rand; bytes=$(( seed % 20000 ))
		rand; case $(( seed % 5 )) in 0) method=POST ;; *) method=GET ;; esac
		rand; case $(( seed % 3 )) in 0) query="?page=$(( seed % 50 ))&sort=asc" ;; *) query= ;; esac
		t=$(( i * 37 )); h=$(( t / 3600 % 24 )) m=$(( t / 60 % 60 )) s=$(( t % 60 ))
		printf '10.%d.%d.%d - - [27/Sep/2026:%02d:%02d:%02d +0000] "%s %s%s HTTP/1.1" %s %d "-" "bench/1.0"\n' \
			"$a" "$b" "$c" "$h" "$m" "$s" "$method" "$path" "$query" "$status" "$bytes"
		i=$(( i + 1 ))
	done
}

# Adds $2 to the counter named $1, remembering new names in $keys.
count() {
	eval "_v=\${$1-}"
	if [ -z "$_v" ]; then
		keys="$keys $1"
		_v=0
	fi
	eval "$1=$(( _v + $2 ))"
}

parse_log() {
	keys=
	oldifs=$IFS
	lines=0 errors=0 total_bytes=0
	while IFS= read -r line; do
		lines=$(( lines + 1 ))
		ip=${line%% *}
		rest=${line#*\"}
		request=${rest%%\"*}
		rest=${rest#*\" }
		status=${rest%% *}
		rest=${rest#* }
		bytes=${rest%% *}
		set -f
		set -- $request
		set +f
		method=$1 path=${2%%\?*}
		case $2 in
		*\?*) count query_requests 1 ;;
		esac
		time=${line#*\[}
		time=${time%%\]*}
		hour=${time#*:}
		hour=${hour%%:*}
		count "hour_$hour" 1
		count "status_$status" 1
		count "method_$method" 1
		case $status in
		[45]*)
			errors=$(( errors + 1 ))
			IFS=.
			set -- $ip
			IFS=$oldifs
			count "subnet_$2_$3" 1
			;;
		esac
		section=${path#/}
		section=${section%%/*}
		case $section in
		'' | *[!a-z0-9]*) section=root ;;
		esac
		count "bytes_$section" "$bytes"
		total_bytes=$(( total_bytes + bytes ))
	done
	printf 'lines %d, errors %d, bytes %d\n' "$lines" "$errors" "$total_bytes"
	for k in $keys; do
		eval "printf '%s %s\n' \"\$k\" \"\$$k\""
	done | sort
}

gen_ini() {
	n=$1
	i=0
	while [ "$i" -lt "$n" ]; do
		printf '[server%d]\n' "$i"
		printf '; generated section %d\n' "$i"
		printf 'host = srv%d.example.org\n' "$i"
		printf 'port=%d\n' $(( 8000 + i % 100 ))
		printf '  enabled = %s\n' "$(( i % 3 != 0 ))"
		printf 'tags = web, db , cache\n\n'
		i=$(( i + 1 ))
	done
}

trim() {
	_t=$1
	_t=${_t#"${_t%%[! 	]*}"}
	_t=${_t%"${_t##*[! 	]}"}
}

parse_ini() {
	section= nkeys=0 enabled=0 portsum=0 ntags=0
	while IFS= read -r line; do
		case $line in
		'' | \;* | \#*) continue ;;
		\[*\]) section=${line#\[}; section=${section%\]}; continue ;;
		*=*) ;;
		*) printf 'bad line: %s\n' "$line"; continue ;;
		esac
		trim "${line%%=*}"; key=$_t
		trim "${line#*=}"; value=$_t
		eval "cfg_${section}_$key=\$value"
		nkeys=$(( nkeys + 1 ))
		case $key in
		enabled) [ "$value" = 1 ] && enabled=$(( enabled + 1 )) ;;
		port) portsum=$(( portsum + value )) ;;
		tags)
			oldifs=$IFS
			IFS=,
			for tag in $value; do
				trim "$tag"
				[ -n "$_t" ] && ntags=$(( ntags + 1 ))
			done
			IFS=$oldifs
			;;
		esac
	done
	printf 'ini: %d keys, %d enabled, port sum %d, %d tags\n' "$nkeys" "$enabled" "$portsum" "$ntags"
	eval "printf 'last host %s\n' \"\$cfg_${section}_host\""
}

gen_log $(( 2000 * scale )) > access.log
parse_log < access.log
gen_ini $(( 500 * scale )) > config.ini
parse_ini < config.ini
