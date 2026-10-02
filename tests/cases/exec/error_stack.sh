# Error messages start with the file of the code that failed (a file read
# with `.`, or the file a function was defined in; dash always names $0) and
# its line, then show the text of that line (read again from the file, or
# from the -c command) and the call stack: a line for each function call and
# file read with `.` that led there, innermost first, with where it was
# called and that line's text. Repeated lines are counted, and a long stack
# loses its middle.
cat > lib.sh <<'X'
load_config() {
  nosuchcmd
  cd /nonexistent
  echo "${unsetvar?is required}"
}
X
cat > main.sh <<'X'
. ./lib.sh
setup() {
  load_config
}
setup
X
echo 'echo "unclosed (' > syntax.sh
echo 'nosuchcmd' > top.sh
echo '--- in functions'
$SH main.sh 2>&1; echo "status $?"
echo '--- at the top level of a script, as in dash'
$SH top.sh 2>&1
echo '--- a file read with . (a syntax error)'
$SH -c 'g() { . ./syntax.sh; }
g' 2>&1
echo '--- in -c, the calls are at lines of the command'
$SH -c '. ./lib.sh
f() { load_config; }
f' 2>&1
echo '--- a function from a saved state has no line in its file'
$SH -c '. ./lib.sh; __luish_internal savestate > state'
$SH -c '. ./state; load_config' 2>&1 | head -2
echo '--- recursion'
ulimit -s 65536 2>/dev/null
$SH -c 'r() { r; }; r' 2>&1 | sed 's/^[^:]*: //'
$SH -c 'f() { g; }
g() { f; }
f' 2>&1 | sed 's/^[^:]*: //'
echo '--- an interactive shell names no lines at the prompt'
$SH -i -c 'g() { nosuchcmd; }; g' 2>&1 </dev/null | grep -v 'job control' | sed 's/^[^:]*: //'
echo '--- the failing line is read again from its file, also after cd'
mkdir sub
$SH -c '. ./lib.sh; cd sub; load_config' 2>&1 | head -2
echo '--- no text for eval, nor line for a function that eval defined'
$SH -c 'eval "a=1
nosuchcmd"; eval "f() { nosuchcmd; }"; f' 2>&1 | sed 's/^[^:]*: //'
echo '--- a long line is cut, and control characters are shown as ?'
printf 'nosuchcmd %0200d\n' 0 > long.sh
printf 'nosuchcmd \001x\n' > ctl.sh
$SH long.sh 2>&1
$SH ctl.sh 2>&1 | tail -1
echo '--- no text for a call made in eval'
$SH -c 'f() { nosuchcmd; }
eval "
f"' 2>&1 | sed 's/^[^:]*: //'
echo '--- the call sites are read again from their files, also after cd'
cat > calls.sh <<'X'
run() {
  cd sub
  load_config
}
X
$SH -c '. ./lib.sh; . ./calls.sh
run' 2>&1 | head -6
