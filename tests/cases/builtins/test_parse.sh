# test parses ambiguous expressions as dash does (its parser was ported):
# every expression of up to four arguments from these tokens, with the
# status and error message (without the lines after it that luish adds:
# the failing line and the call stack). E stands for the empty string.
touch f
toks='! -a -o ( ) -n x = -f -eq 1 E'
t() {
  [ "$@" ] 2>&1
  echo "$? $*"
}
for a in $toks; do
  [ "$a" = E ] && a=
  t "$a"
  for b in $toks; do
    [ "$b" = E ] && b=
    t "$a" "$b"
    for c in $toks; do
      [ "$c" = E ] && c=
      t "$a" "$b" "$c"
      for d in $toks; do
        [ "$d" = E ] && d=
        t "$a" "$b" "$c" "$d"
      done
    done
  done
done | sed '/^  /d'
t
[ -a -a -a -a -a ]; echo $?
[ ! -a -a -a -a -a ]; echo $?
[ x -a ! -z -a -o ! x ]; echo $?
[ f -nt nonexistent ]; echo $?
[ nonexistent -ot f ]; echo $?
