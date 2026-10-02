# bash's BASH_SOURCE: the files being run, innermost first. `.` adds the
# file (as found, with its directory if found in PATH), and a function call
# the file the function was defined in. It is unset in -c and on standard
# input, and a function defined there has an empty file (bash: "main" or
# "environment").
echo "main: [$BASH_SOURCE] [${BASH_SOURCE[0]}] ${#BASH_SOURCE[@]}"
# What called `show` (its own entry is the first).
show() { echo "$1: [${BASH_SOURCE[*]:1}]"; }
show call
mkdir -p lib bin
cat > lib/a.sh <<'X'
show "a.sh"
a_fn() { show a_fn; }
a_src() { . ./lib/b.sh; }
X
echo 'show b.sh' > lib/b.sh
echo 'show path.sh' > bin/path.sh
. ./lib/a.sh
a_fn
a_src
outer() { a_fn; }
outer
(a_fn)
echo "$(a_fn)"
eval a_fn
PATH=$PWD/bin:$PATH
. path.sh | sed "s|$PWD|PWD|"
# Run or sourced?
cat > lib/which.sh <<'X'
if [ "${BASH_SOURCE[0]}" = "$0" ]; then echo run; else echo sourced; fi
X
$SH lib/which.sh
. ./lib/which.sh
# -c and standard input.
$SH -c 'echo "c: [${BASH_SOURCE-unset}] ${#BASH_SOURCE[@]}"; f() { echo "f: ${#BASH_SOURCE[@]} [${BASH_SOURCE[*]}]"; }; f
show() { echo "$1: [${BASH_SOURCE[*]:1}]"; }; . ./lib/a.sh; a_fn'
echo 'echo "stdin: [${BASH_SOURCE-unset}]"; show() { echo "$1: [${BASH_SOURCE[*]:1}]"; }; . ./lib/b.sh; show stdin' > lib/stdin.sh
$SH < lib/stdin.sh
$SH -uc 'echo $BASH_SOURCE' 2>/dev/null || echo "unset: $?"
# The directory of a script, through a symbolic link, with modifiers.
mkdir real
echo 'echo "dir: ${BASH_SOURCE:A:h:t} ${BASH_SOURCE[0]:h}"' > real/s.sh
ln -s real/s.sh link.sh
$SH ./link.sh
. ./link.sh
# Assigning makes it an ordinary variable, as for the other specials.
typeset -p BASH_SOURCE
(BASH_SOURCE=x; echo "assigned: $BASH_SOURCE ${#BASH_SOURCE[@]}"; a_fn)
(unset BASH_SOURCE; echo "unset: ${BASH_SOURCE-unset}")
# Saving the state keeps the file of each function.
__luish_internal savestate > state
grep -c 'function-file' state
$SH -c '. ./state; a_fn'
