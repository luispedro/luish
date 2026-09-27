# A make-like build driver: generates a source tree, scans it for
# dependencies with `while read`, "compiles" each file by running a recipe
# with `$SH -c` (as make does, so this also measures the shell's start-up),
# links the results with pipelines, then does an incremental rebuild.
# Uses command substitution for basename and dirname as many scripts do,
# globbing, `find`, subshells with `cd`, and redirections.
#
# $SH is the shell under test, possibly with arguments (it is split).

scale=${1:-1}
SH=${SH:-sh}
work=$(mktemp -d) || exit 1
trap 'rm -rf "$work"' EXIT
cd "$work" || exit 1

nmods=$(( 4 * scale ))
nfiles=12

gen_tree() {
	m=0
	while [ "$m" -lt "$nmods" ]; do
		dir=src/mod$m
		mkdir -p "$dir" include
		printf '#define MOD%d_VERSION %d\n' "$m" "$m" > "include/mod$m.h"
		f=0
		while [ "$f" -lt "$nfiles" ]; do
			{
				printf '/* mod%d/file%d.c */\n' "$m" "$f"
				printf '#include "mod%d.h"\n' "$m"
				[ "$f" -gt 0 ] && printf '#include "mod%d.h"\n' $(( (m + f) % nmods ))
				printf '#include <stdio.h>\n'
				i=0
				while [ "$i" -lt 20 ]; do
					printf 'int fn_%d_%d_%d(int x) { return x * %d + MOD%d_VERSION; }\n' \
						"$m" "$f" "$i" "$i" "$m"
					i=$(( i + 1 ))
				done
			} > "$dir/file$f.c"
			f=$(( f + 1 ))
		done
		m=$(( m + 1 ))
	done
}

# Writes the dependencies of $1, one per line, to $1.d.
scan_deps() {
	while IFS= read -r line; do
		case $line in
		'#include "'*'"')
			h=${line#*\"}
			printf 'include/%s\n' "${h%\"}"
			;;
		esac
	done < "$1" > "${1%.c}.d"
}

# A recipe, run in a new shell per object as make does.
recipe='mkdir -p "$(dirname "$2")" &&
sed -n "s/^int \(fn_[0-9_]*\)(.*/T \1/p" "$1" > "$2.tmp" &&
{ printf "%s\n" "# object for $1"; cat "$2.tmp"; } > "$2" &&
rm -f "$2.tmp"'

needs_build() {
	obj=$1 src=$2
	[ -f "$obj" ] || return 0
	while IFS= read -r dep; do
		[ "$dep" -nt "$obj" ] && return 0
	done < "${src%.c}.d"
	return 1
}

build() {
	built=0 skipped=0
	for src in src/*/*.c; do
		name=$(basename "$src" .c)
		mod=$(basename "$(dirname "$src")")
		obj=build/$mod/$name.o
		if needs_build "$obj" "$src"; then
			$SH -c "$recipe" recipe "$src" "$obj" || return 1
			built=$(( built + 1 ))
		else
			skipped=$(( skipped + 1 ))
		fi
	done
	for d in build/*/; do
		mod=${d%/}
		mod=${mod##*/}
		cat "$d"*.o | grep -v '^#' | sort > "build/lib$mod.syms"
	done
	syms=$(cat build/lib*.syms | wc -l)
	printf 'built %d, up to date %d, %d symbols\n' "$built" "$skipped" "$syms"
}

gen_tree
for src in src/*/*.c; do
	scan_deps "$src"
done
printf 'sources: %d, dependency lines: %d\n' \
	"$(find src -name '*.c' | wc -l)" "$(cat src/*/*.d | wc -l)"

build
build

# Remove some objects, as if their sources changed, and rebuild.
( cd build && for o in */file3.o */file7.o; do rm -f "$o"; done )
build

printf 'tree: %s\n' "$(find build -type f -name '*.o' | sort | xargs cat | cksum)"
