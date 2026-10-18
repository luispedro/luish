# `basename` and `dirname` run in the shell when they are the system's and
# their arguments are plain (`exec/utility.rs`); `$(basename ...)` without a
# subshell too. Everything must look as if the utility ran.

for s in / // /// a a/ a// /a /a/ //a //a/ ///a a/b a/b/ a//b// /a//b //a//b \
    /a/b/c.txt 'a b/c d' . .. ./ ../x '' - -- '*' 'a
b/c
'; do
    printf '[%s] [%s] [%s]\n' "$(basename "$s")" "$(dirname -- "$s")" "$s"
    basename -- "$s"; dirname -- "$s"
done
for p in 'a.txt .txt' '.txt .txt' 'a/.txt .txt' 'a.txt/ .txt' 'a.txt t' "a.txt ''" '/ /' 'x/ x' \
    'a.b.c .c' 'a/b b' 'abc bc/' "a ''"; do
    eval "set -- $p"
    echo "[$(basename "$1" "$2")] [$(basename -- "$1" "$2")]"
    basename "$1" "$2"
done
f=/usr/share/doc/README.md
echo "$(basename "$f" .md) $(dirname "$f") $(basename ${f%.md}) $(dirname "${f#/usr}")"
echo `basename $f` `dirname $f`
printf '%s|' $(basename "a/b c"); echo
basename "$(printf 'a/\377\376')" | od -An -c
x=$(dirname "$(printf '\377/\376')"); printf %s "$x" | od -An -c

# The status, and `$?` after an assignment, as for any `$(...)`.
false; x=$(basename /a/b); echo "status $? $x"
false; x=$(dirname a b c); echo "status $? $x"
x=$(basename); echo "status $? [$x]"
x=$(basename a b c); echo "status $? [$x]"
basename 2>/dev/null; echo "status $?"
x=$(basename /a/b >/dev/null); echo "status $? [$x]"

# Options, and output to a pipe: the utility runs.
echo "$(basename -s .c a/b.c) $(basename -a x/y z/w | tr '\n' ' ')"
basename x/y | cat
basename a/b >&- 2>/dev/null; echo "closed $?"

# A function, or a command of one's own in PATH, comes first.
basename() { echo "function $*"; }
echo "$(basename /a/b)"; basename /a/b
unset -f basename
mkdir bin
printf '#!/bin/sh\necho mine "$@"\n' > bin/dirname
chmod +x bin/dirname
p=$PATH
PATH=$PWD/bin:$PATH
echo "$(dirname /a/b)"; dirname /a/b
PATH=$p
echo "$(dirname /a/b)"
PATH=/nowhere
x=$(basename /a/b 2>/dev/null); echo "missing $? [$x]"
basename /a/b 2>/dev/null; echo "missing $?"
PATH=$p

# The words are expanded as in a subshell.
hash -r
x=$(basename /a/b); hash
set -- one/two 'three /four'
echo "$(basename "$1") $(basename $2) $(dirname "$@") $(basename "$#")"
x=$(basename *); echo "glob [$x]"
y=$(dirname ~/a); [ "$y" = "$HOME" ] && echo home
set -u
x=$(basename "$nope" 2>/dev/null); echo "unset $? [$x]"
set +u
x=$(basename "${nope?gone}" 2>/dev/null); echo "error $? [$x]"
x=$(basename "${v=set}"); echo "assign [$x] [${v-unset}]"
i=0; x=$(basename "$((i += 1))"); echo "arith [$x] $i"
x=$(basename "$(echo a/b)"); echo "nested [$x]"

# A `$(...)` in the words runs once, and can't change the shell, also when
# the utility has to run or isn't the system's.
x=$(basename "$(dirname "$(dirname /p/q/r/s)")"); echo "nested twice [$x]"
x=$(basename "$(echo run >> log; echo a/b)"); echo "once [$x] $(wc -l < log)"
x=$(basename -a "$(echo run >> log; echo c/d)" "$(echo e/f)"); echo "utility [$x] $(wc -l < log)"
x=$(basename "$(exit 3)"); echo "empty $? [$x]"
false; x=$(dirname "$(false)"/x); echo "status $? [$x]"
x=$(basename "$(y=1; cd /; echo a/$y)"); echo "subshell [$x] [${y-unset}] $([ "$PWD" != / ] && echo here)"
x=$(expr "$(echo 2)" + "$(echo 3)"); echo "expr [$x]"
x=$(basename "${y-$(echo run >> log; echo g/h)}"); echo "default [$x] $(wc -l < log)"
PATH=$PWD/bin:$PATH
x=$(dirname "$(echo run >> log; echo /a/b)"); echo "own [$x] $(wc -l < log)"
PATH=$p
