# Deviation from dash: exec accepts -- before the command (as POSIX
# requires and bash does); dash tries to run a command named --.
exec -- 3>&1
echo stdout 1>&3
$SH -c 'exec -- echo hi'
echo "status $?"
