# bash's options of `read` (the expected output is bash's).
show() { printf '<%s>' "$@"; echo; }
# Without a name: REPLY gets the line as it is (zsh strips IFS whitespace).
printf '  a \\ b  \n' | { read; show "$?" "$REPLY"; }
printf '  a \\ b  \n' | { read -r; show "$REPLY"; }
printf 'x' | { read; show "$?" "$REPLY"; }
# -d: the first character of its argument ends the line instead of a
# newline; -d '' is NUL. A backslash quotes it, and backslash-newline is
# still dropped.
printf 'a b:c\n' | { read -d : x y; show "$?" "$x" "$y"; }
printf 'a\\:b\\\nc:' | { read -d :x v; show "$?" "$v"; }
printf 'one\ntwo\0three\0' | { read -r -d '' v; show "$?" "$v"; read -d '' v; show "$?" "$v"; }
printf 'a\\\0b\0' | { read -d '' v; show "$v"; }
printf 'ab\n' | { read -d ''; show "$?" "$REPLY"; }
printf 'a b\n' | { read -d '' x y; show "$?" "$x" "$y"; }
printf 'f1\0f 2\0' | while IFS= read -r -d '' f; do show "$f"; done
# -n: at most so many characters (escaped ones count, the backslash
# doesn't), or up to the delimiter; the text is split as usual.
printf 'abcdef\n' | { read -n 3 v; read w; show "$?" "$v" "$w"; }
printf 'a\\bc\n' | { read -n 2 v; show "$v"; }
printf 'a\\\nbcd\n' | { read -n 3 v; show "$v"; }
printf 'ab\ncd\n' | { read -n 5 v; read w; show "$v" "$w"; }
printf ' a  b c\n' | { read -n 6 x y; show "$x" "$y"; }
printf 'xyz' | { read -n 5 v; show "$?" "$v"; }
printf 'abc\n' | { read -n 0 v; read w; show "$?" "$v" "$w"; }
printf 'ab\\' | { read -n 3 v; show "$?" "$v"; }
printf 'ab\n' | { read -n 1 -d '' v; show "$v"; }
# -N: exactly so many, with the delimiter an ordinary character and
# nothing split; NUL bytes and backslash-newline don't count.
printf 'a b\ncd\n' | { read -N 4 v; show "$?" "$v"; }
printf ' ab cd ef' | { read -N 6 x y; show "$x" "$y"; }
printf 'a:b\n' | { IFS=: read -N 3 x y; show "$x" "$y"; }
printf 'a\0b\0c' | { read -N 2 v; show "$v"; }
printf 'a\\\nbcd' | { read -N 3 v; show "$v"; }
printf 'a\\\nbcd' | { read -rN 3 v; show "$v"; }
printf 'ab' | { read -N 3 v; show "$?" "$v"; }
printf 'a b c' | { read -N 5 -a arr; show "${#arr[@]}" "${arr[@]}"; }
# The last of -n and -N counts.
printf 'abc\n' | { read -N 2 -n 1 v; show "$v"; }
printf 'abc\n' | { read -n 1 -N 2 v; show "$v"; }
# Grouped, with the argument in the same word.
printf 'abc\n' | { read -rn2 v; show "$v"; }
# -u: another fd.
printf 'from 3\n' >f3
printf 'from 0\n' | { read -u 3 x 3<f3; read y; show "$x" "$y"; }
exec 4<f3
read -u4 v
show "$v"
exec 4<&-
# -t: a timeout, which may have a fraction; status 142 when it runs out,
# with what was read so far assigned.
printf 'x\n' | { read -t 1.5 v; show "$?" "$v"; }
printf 'x\n' | { read -t .5 v; show "$?" "$v"; }
printf 'x\n' | { read -t 1. v; show "$?" "$v"; }
v=old; sleep 1 | { read -t 0.05 v; show "$?" "$v"; }
{ printf 'a b'; sleep 1; } | { read -t 0.1 x y; show "$?" "$x" "$y"; }
{ printf 'part'; sleep 1; } | { read -t 0.1; show "$?" "$REPLY"; }
# -t 0: only whether there is input (the end of it counts), nothing read.
read -t 0 <f3; show "$?"
read -t 0 </dev/null; show "$?"
v=old; read -t 0 v <f3; show "$?" "$v"
sleep 1 | { read -t 0; show "$?"; }
# -s, -e and -i do nothing without a terminal; -p prompts only on one.
printf 'a b\n' | { read -s -e -i x -p 'prompt' v w; show "$v" "$w"; }
# Errors.
read -n x v </dev/null; show "$?"
read -n -1 v </dev/null; show "$?"
read -n 99999999999999999999 v </dev/null; show "$?"
read -N 2147483648 v </dev/null; show "$?"
read -t x v </dev/null; show "$?"
read -t -1 v </dev/null; show "$?"
read -t 1e1 v </dev/null; show "$?"
read -u x v; show "$?"
read -u '' v; show "$?"
read -u 7 v; show "$?"
read -d 2>/dev/null; show "$?"
read -z </dev/null; show "$?"
