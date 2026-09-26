# local is a special built-in in dash: prefix assignments persist, and
# errors (and redirection errors) exit the shell.
f() {
  E=env local v=var
  echo $E $v
}
f
$SH -c 'f() { local x=1 >/nonexistent/a; echo after; }; f; echo notreached' 2>/dev/null
echo "status $?"
$SH -c 'local x; echo notreached' 2>/dev/null
echo "status $?"
$SH -c 'f() { local 1x; echo notreached; }; f' 2>/dev/null
echo "status $?"
$SH -c 'f() { command local 1x; echo "reached $?"; }; f' 2>/dev/null
type local
