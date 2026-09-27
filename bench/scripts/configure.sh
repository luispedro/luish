# An autoconf-style `configure`: option parsing, cached checks for headers,
# functions and types (each writing a conftest file with a here-document,
# logging to config.log on fd 5, and "compiling" it with grep against fake
# system headers), confdefs.h accumulation, and a config.status step that
# builds a sed script and substitutes templates. Written in the style of
# generated autoconf code (as_fn_* helpers, `eval` on cache variables,
# `sed` to sanitise names), so it mixes built-ins with many small forks.

scale=${1:-1}
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
cd "$work" || exit 1

as_nl='
'
export as_nl
as_me=configure

as_fn_error() {
	as_status=$1
	test "$as_status" -eq 0 && as_status=1
	printf '%s\n' "$as_me: error: $2" >&2
	exit "$as_status"
}

as_fn_append() {
	eval $1=\$$1\$2
}

as_fn_arith() {
	as_val=$(( $* ))
}

# Like autoconf's as_tr_sh and as_tr_cpp.
as_tr_sh="eval sed 'y%*+%pp%;s%[^_a-zA-Z0-9]%_%g'"
as_tr_cpp="eval sed 'y%*abcdefghijklmnopqrstuvwxyz%PABCDEFGHIJKLMNOPQRSTUVWXYZ%;s%[^_A-Za-z0-9]%_%g'"

ac_fn_try_compile() {
	# "Compiles" conftest.c: every header it includes must exist under sysinc,
	# and every symbol it names must be listed in sysinc/symbols.
	ac_retval=0
	while IFS= read -r ac_line; do
		case $ac_line in
		'#include <'*'>')
			ac_hdr=${ac_line#*<}
			ac_hdr=${ac_hdr%>}
			test -f "sysinc/$ac_hdr" || ac_retval=1
			;;
		'/* check: '*' */')
			ac_sym=${ac_line#'/* check: '}
			ac_sym=${ac_sym%' */'}
			grep "^$ac_sym\$" sysinc/symbols >/dev/null 2>&1 || ac_retval=1
			;;
		esac
	done < conftest.c
	printf '%s\n' "configure:$ac_lineno: result of compiling: $ac_retval" >&5
	if test $ac_retval -ne 0; then
		printf '%s\n' "configure: failed program was:" >&5
		sed 's/^/| /' conftest.c >&5
	fi
	rm -f conftest.c conftest.o
	return $ac_retval
}

ac_check_header() {
	ac_lineno=$(( ac_lineno + 1 ))
	as_ac_Header=$(printf '%s\n' "ac_cv_header_$1" | $as_tr_sh)
	printf '%s' "checking for $1... " >&6
	if eval test \"\${$as_ac_Header+set}\" = set; then
		printf '%s' "(cached) " >&6
	else
		cat confdefs.h - > conftest.c <<_ACEOF
/* end confdefs.h.  */
$ac_includes_default
#include <$1>
_ACEOF
		if ac_fn_try_compile; then
			eval "$as_ac_Header=yes"
		else
			eval "$as_ac_Header=no"
		fi
	fi
	eval ac_res=\$$as_ac_Header
	printf '%s\n' "$ac_res" >&6
	if test "$ac_res" = yes; then
		cat >> confdefs.h <<_ACEOF
#define $(printf '%s\n' "HAVE_$1" | $as_tr_cpp) 1
_ACEOF
	fi
}

ac_check_func() {
	ac_lineno=$(( ac_lineno + 1 ))
	as_ac_var=$(printf '%s\n' "ac_cv_func_$1" | $as_tr_sh)
	printf '%s' "checking for $1... " >&6
	if eval test \"\${$as_ac_var+set}\" = set; then
		printf '%s' "(cached) " >&6
	else
		cat confdefs.h - > conftest.c <<_ACEOF
/* end confdefs.h.  */
/* Define $1 to an innocuous variant, in case <limits.h> declares $1. */
#define $1 innocuous_$1
#include <limits.h>
#undef $1
char $1 ();
/* check: $1 */
int main () { return $1 (); }
_ACEOF
		if ac_fn_try_compile; then
			eval "$as_ac_var=yes"
		else
			eval "$as_ac_var=no"
		fi
	fi
	eval ac_res=\$$as_ac_var
	printf '%s\n' "$ac_res" >&6
	if test "$ac_res" = yes; then
		cat >> confdefs.h <<_ACEOF
#define $(printf '%s\n' "HAVE_$1" | $as_tr_cpp) 1
_ACEOF
	fi
}

ac_check_sizeof() {
	ac_lineno=$(( ac_lineno + 1 ))
	as_ac_Sizeof=$(printf '%s\n' "ac_cv_sizeof_$1" | $as_tr_sh)
	printf '%s' "checking size of $1... " >&6
	case $1 in
	char) ac_size=1 ;;
	short) ac_size=2 ;;
	int | float) ac_size=4 ;;
	*) ac_size=8 ;;
	esac
	eval "$as_ac_Sizeof=\$ac_size"
	printf '%s\n' "$ac_size" >&6
	cat >> confdefs.h <<_ACEOF
#define $(printf '%s\n' "SIZEOF_$1" | $as_tr_cpp) $ac_size
_ACEOF
}

# The fake system: headers and symbols, some of them missing.
make_system() {
	mkdir -p sysinc/sys sysinc/net sysinc/arpa
	for h in $headers; do
		case $h in
		*nothere* | *missing*) ;;
		*) : > "sysinc/$h" ;;
		esac
	done
	for f in $functions; do
		case $f in
		*_nope) ;;
		*) printf '%s\n' "$f" ;;
		esac
	done > sysinc/symbols
}

headers='stdio.h stdlib.h string.h strings.h inttypes.h stdint.h unistd.h
sys/types.h sys/stat.h sys/time.h sys/wait.h sys/socket.h sys/select.h
sys/param.h sys/resource.h sys/mman.h sys/ioctl.h sys/nothere.h netinet_missing.h
fcntl.h errno.h limits.h locale.h langinfo.h wchar.h wctype.h termios.h
signal.h dirent.h pwd.h grp.h poll.h pthread.h dlfcn.h net/if.h arpa/inet.h
malloc_missing.h alloca.h'
functions='malloc realloc free memcpy memmove memset strchr strrchr strdup
strndup strerror strsignal strtol strtoul strtoll strtoull setlocale nl_langinfo
mbrtowc wcrtomb wcwidth iswprint fork vfork waitpid posix_spawn pipe2 dup2 dup3
fcntl open openat close read write lseek fstat lstat stat fstatat readlink
getcwd chdir fchdir mkdir mkdtemp rmdir unlink rename symlink getpwnam
getgrgid select poll ppoll sigaction sigprocmask kill raise alarm setitimer
clock_gettime gettimeofday nanosleep strftime localtime_r gmtime_r
arc4random_nope getrandom explicit_bzero_nope reallocarray qsort_r_nope'
types='char short int long float double size_t off_t pid_t time_t'

# Command-line options, parsed as configure does.
ac_args="--prefix=/opt/bench --exec-prefix=/opt/bench --enable-shared
--disable-static --enable-nls --with-pic --without-readline --with-zlib=/usr
--enable-debug=no --enable-silent-rules --libdir=/opt/bench/lib64 CFLAGS=-O2
--host=x86_64-pc-linux-gnu --build=x86_64-pc-linux-gnu --cache-file=/dev/null
--enable-threads=posix --disable-rpath --with-sysroot=/ --enable-largefile"

parse_options() {
	prefix=NONE exec_prefix=NONE libdir='${exec_prefix}/lib' ac_features= ac_packages=
	ac_prev=
	for ac_option in $ac_args; do
		if test -n "$ac_prev"; then
			eval $ac_prev=\$ac_option
			ac_prev=
			continue
		fi
		case $ac_option in
		*=?*) ac_optarg=$(expr "X$ac_option" : '[^=]*=\(.*\)') ;;
		*=) ac_optarg= ;;
		*) ac_optarg=yes ;;
		esac
		case $ac_option in
		--prefix=*) prefix=$ac_optarg ;;
		--exec-prefix=*) exec_prefix=$ac_optarg ;;
		--libdir=*) libdir=$ac_optarg ;;
		--host=* | --build=* | --cache-file=* | --with-sysroot=*) ;;
		--enable-* | --disable-*)
			ac_useropt=${ac_option#--enable-}
			ac_useropt=${ac_useropt#--disable-}
			ac_useropt=${ac_useropt%%=*}
			ac_useropt=$(printf '%s\n' "$ac_useropt" | sed 's/[-+.]/_/g')
			case $ac_option in
			--disable-*) ac_optarg=no ;;
			esac
			eval enable_$ac_useropt=\$ac_optarg
			as_fn_append ac_features " $ac_useropt"
			;;
		--with-* | --without-*)
			ac_useropt=${ac_option#--with-}
			ac_useropt=${ac_useropt#--without-}
			ac_useropt=${ac_useropt%%=*}
			ac_useropt=$(printf '%s\n' "$ac_useropt" | sed 's/[-+.]/_/g')
			case $ac_option in
			--without-*) ac_optarg=no ;;
			esac
			eval with_$ac_useropt=\$ac_optarg
			as_fn_append ac_packages " $ac_useropt"
			;;
		*=*)
			ac_envvar=${ac_option%%=*}
			eval "$ac_envvar=\$ac_optarg"
			export "$ac_envvar"
			;;
		*) as_fn_error 1 "unrecognized option: \`$ac_option'" ;;
		esac
	done
	test "$exec_prefix" = NONE && exec_prefix=$prefix
}

config_status() {
	# Turn confdefs.h into a sed script, like config.status does.
	sed -n 's/^#define \([A-Za-z_][A-Za-z0-9_]*\) \(.*\)/s|^#undef \1$|#define \1 \2|/p' \
		confdefs.h > defines.sed
	ac_n=0
	{
		printf '/* config.h.in */\n'
		for h in $headers; do
			printf '#undef HAVE_%s\n' "$h"
		done | $as_tr_cpp | sed 's/^_UNDEF_/#undef /'
		for f in $functions; do
			printf '#undef HAVE_%s\n' "$f"
		done | $as_tr_cpp | sed 's/^_UNDEF_/#undef /'
		for t in $types; do
			printf '#undef SIZEOF_%s\n' "$t"
		done | $as_tr_cpp | sed 's/^_UNDEF_/#undef /'
	} > config.h.in
	sed -f defines.sed config.h.in | sed 's|^#undef \(.*\)|/* #undef \1 */|' > config.h
	cat > Makefile.in <<'_EOF'
prefix = @prefix@
exec_prefix = @exec_prefix@
libdir = @libdir@
CFLAGS = @CFLAGS@
DEFS = @DEFS@
FEATURES = @FEATURES@
PACKAGES = @PACKAGES@
all: bench
_EOF
	ac_defs=$(sed -n 's/^#define \([A-Za-z_0-9]*\) .*/-D\1/p' confdefs.h | tr '\n' ' ')
	sed -e "s|@prefix@|$prefix|g" -e "s|@exec_prefix@|$exec_prefix|g" \
		-e "s|@libdir@|$libdir|g" -e "s|@CFLAGS@|$CFLAGS|g" \
		-e "s|@DEFS@|$ac_defs|g" -e "s|@FEATURES@|$ac_features|g" \
		-e "s|@PACKAGES@|$ac_packages|g" Makefile.in > Makefile
}

ac_run=0
while test "$ac_run" -lt "$scale"; do
	rm -f confdefs.h config.log
	exec 5> config.log 6> checks.log
	ac_lineno=0
	cat > confdefs.h <<_ACEOF
/* confdefs.h */
#define PACKAGE_NAME "bench"
#define PACKAGE_VERSION "1.0"
_ACEOF
	ac_includes_default='#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>'
	parse_options
	make_system
	for ac_header in $headers; do
		ac_check_header "$ac_header"
	done
	# Checking again finds the cached results.
	for ac_header in stdio.h stdlib.h string.h unistd.h; do
		ac_check_header "$ac_header"
	done
	for ac_func in $functions; do
		ac_check_func "$ac_func"
	done
	for ac_type in $types; do
		ac_check_sizeof "$ac_type"
	done
	config_status
	exec 5>&- 6>&-
	for var in $(set | sed -n 's/^\(ac_cv_[a-z_0-9]*\)=.*/\1/p'); do
		unset "$var"
	done
	ac_run=$(( ac_run + 1 ))
done

printf 'checks: %s\n' "$(grep -c . checks.log)"
printf 'yes: %s, no: %s\n' "$(grep -c 'yes$' checks.log)" "$(grep -c 'no$' checks.log)"
printf 'cached: %s\n' "$(grep -c cached checks.log)"
printf 'config.log: %s lines\n' "$(wc -l < config.log | tr -d ' ')"
printf 'config.h: %s\n' "$(cksum < config.h)"
printf 'Makefile: %s\n' "$(cksum < Makefile)"
sed -n 's/^\(prefix\|libdir\|FEATURES\|PACKAGES\) = /\1: /p' Makefile
