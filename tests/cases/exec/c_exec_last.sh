# reference: zsh
# The last command of a -c string replaces the shell (as in upstream dash;
# Debian's dash patches this out).
echo $$ > me
$SH -c "$SH -c 'echo \$PPID > parent'"
[ "$(cat parent)" = "$(cat me)" ] && echo "last command exec'd"
$SH -c ": ; $SH -c 'echo \$PPID > parent'"
[ "$(cat parent)" = "$(cat me)" ] && echo "last of several exec'd"
$SH -c "$SH -c 'echo \$PPID > parent'; :"
[ "$(cat parent)" != "$(cat me)" ] && echo "not last: forked"
$SH -c "$SH -c 'echo \$PPID > parent' # comment"
[ "$(cat parent)" = "$(cat me)" ] && echo "comment after it: exec'd"
$SH -c 'trap "echo exit trap" EXIT; /bin/echo trap set: forked'
$SH -c '/bin/sh -c "exit 5"'; echo "status: $?"
