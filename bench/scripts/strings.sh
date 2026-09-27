# String manipulation in pure shell, as done by shell libraries that avoid
# forking tr/sed/awk: character-by-character loops, replace-all with pattern
# removal, URL and JSON escaping, splitting with IFS, word frequencies in
# eval'd variables, and printf formatting. The only external commands are
# mktemp, rm, sort and cksum.

scale=${1:-1}
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
cd "$work" || exit 1

# Upper-cases $1 into $REPLY, one character at a time.
upper() {
	_s=$1 REPLY=
	while [ -n "$_s" ]; do
		_r=${_s#?}
		_c=${_s%"$_r"}
		_s=$_r
		case $_c in
		a) _c=A ;; b) _c=B ;; c) _c=C ;; d) _c=D ;; e) _c=E ;; f) _c=F ;;
		g) _c=G ;; h) _c=H ;; i) _c=I ;; j) _c=J ;; k) _c=K ;; l) _c=L ;;
		m) _c=M ;; n) _c=N ;; o) _c=O ;; p) _c=P ;; q) _c=Q ;; r) _c=R ;;
		s) _c=S ;; t) _c=T ;; u) _c=U ;; v) _c=V ;; w) _c=W ;; x) _c=X ;;
		y) _c=Y ;; z) _c=Z ;;
		esac
		REPLY=$REPLY$_c
	done
}

# Reverses $1 into $REPLY.
reverse() {
	_s=$1 REPLY=
	while [ -n "$_s" ]; do
		_r=${_s#?}
		REPLY=${_s%"$_r"}$REPLY
		_s=$_r
	done
}

# Replaces every $2 in $1 with $3, into $REPLY.
replace_all() {
	_s=$1 REPLY=
	while :; do
		case $_s in
		*"$2"*)
			REPLY=$REPLY${_s%%"$2"*}$3
			_s=${_s#*"$2"}
			;;
		*) break ;;
		esac
	done
	REPLY=$REPLY$_s
}

json_escape() {
	replace_all "$1" '\' '\\'
	replace_all "$REPLY" '"' '\"'
	replace_all "$REPLY" "	" '\t'
}

# URL-encodes $1 into $REPLY (ASCII only).
urlencode() {
	_s=$1 REPLY=
	while [ -n "$_s" ]; do
		_r=${_s#?}
		_c=${_s%"$_r"}
		_s=$_r
		case $_c in
		[a-zA-Z0-9.~_-]) REPLY=$REPLY$_c ;;
		' ') REPLY=$REPLY+ ;;
		*) REPLY=$REPLY$(printf '%%%02X' "'$_c") ;;
		esac
	done
}

words='the quick brown fox jumps over lazy dog while shell scripts parse "quoted" text
with tabs	and back\slashes plus symbols like & and = and ? and some repeated repeated words
level noon radar lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor'

gen_text() {
	n=$1
	set -f
	set -- $words
	set +f
	nw=$#
	seed=7 i=0 line=
	while [ "$i" -lt "$n" ]; do
		seed=$(( (seed * 75 + 74) % 65537 ))
		shift $(( seed % $# ))
		line="$line $1"
		set -f
		set -- $words
		set +f
		case $(( i % 12 )) in
		11) printf '%s\n' "${line# }"; line= ;;
		esac
		i=$(( i + 1 ))
	done
	[ -n "$line" ] && printf '%s\n' "${line# }"
}

process() {
	keys= nlines=0 nchars=0 palins=0
	while IFS= read -r line; do
		nlines=$(( nlines + 1 ))
		nchars=$(( nchars + ${#line} ))
		upper "$line"
		up=$REPLY
		json_escape "$line"
		printf '{"n":%d,"text":"%s","upper":"%s"}\n' "$nlines" "$REPLY" "$up" >> out.json
		case $(( nlines % 4 )) in
		0)
			urlencode "$line"
			printf '%s\n' "$REPLY" >> urls.txt
			;;
		esac
		set -f
		for w in $line; do
			case $w in
			*[!a-z]*) continue ;;
			esac
			eval "_n=\${w_$w-}"
			[ -z "$_n" ] && keys="$keys $w"
			eval "w_$w=$(( ${_n:-0} + 1 ))"
			reverse "$w"
			[ "$REPLY" = "$w" ] && palins=$(( palins + 1 ))
		done
		set +f
	done
	printf 'lines %d, chars %d, palindromic words %d\n' "$nlines" "$nchars" "$palins"
	for k in $keys; do
		eval "printf '%-12s %5d\n' \"\$k\" \"\$w_$k\""
	done | sort
	printf 'json: %s\n' "$(cksum < out.json)"
	printf 'urls: %s\n' "$(cksum < urls.txt)"
}

gen_text $(( 5000 * scale )) > text.txt
process < text.txt
