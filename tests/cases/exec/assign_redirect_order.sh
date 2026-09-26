# Words are expanded, then redirections made, then assignments expanded
# (XCU 2.9.1), so a command substitution in an assignment sees the
# redirections.
foo=`cat`<<EOM
hello world
EOM
echo "$foo"
paths=`tr '\n' ':' | sed -e 's/:$//'`<<EOPATHS
/foo
/bar
EOPATHS
echo "$paths"
x=$(echo out; echo err >&2) 2>/dev/null >/dev/null
echo "x=$x"
x=$(false) >/dev/null; echo "status $?"
x=$(false) >/nonexistent/f; echo "status $?"
abc=def >/nonexistent/f
echo "abc=$abc"
rm -f walrus
$SH -c 'X=${x?bc} > walrus' 2>/dev/null
test -f walrus && echo exists1
rm -f walrus
$SH -c '>walrus echo ${a?bc}' 2>/dev/null
test -f walrus && echo exists2
# set -x traces go to the stderr the command started with.
{ set -x; echo hi 2>/dev/null; y=$(echo a) 2>/dev/null; set +x; } 2>&1
# An expansion error in a redirection is fatal; a failed open is not.
$SH -c 'echo hi > file$(( 42 / 0 )); echo inside=$?' 2>/dev/null
echo outside=$?
$SH -c 'cat </nonexistent; echo inside=$?; f() { :; }; f >/nonexistent/f; echo inside=$?' 2>/dev/null
# A redirection error on a special built-in is fatal.
$SH -c 'export abc=def >/nonexistent/f; echo notreached' 2>/dev/null
echo "status $?"
