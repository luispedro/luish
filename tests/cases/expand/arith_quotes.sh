# Arithmetic: a variable holding only blanks is 0, and (as in dash) quotes
# and backslashes inside $((...)) are errors.
a=' '; echo $((a))
b=' 3 '; echo $((b + 1))
c=''; echo $((c))
x=1
echo $(( ${x} + 2 )) $(($x+1))
for e in '"1" + 2' "'1' + 2" '"$x" + 2' '1 \+ 2' 'x"" + 1'; do
  $SH -c "x=1; echo \$(( $e ))" 2>&1 | sed '/^  /d; s/^[^:]*: [0-9]*: //'
  echo "status $?"
done
