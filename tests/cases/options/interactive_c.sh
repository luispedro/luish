# -i with -c runs the command string as an interactive shell (it does not
# read stdin). Deviation from dash: the emacs option is on in interactive
# shells (the line editor's mode), so $- has E.
$SH -i -c 'echo "$-"; set -u; echo before; echo $x; echo after' </dev/null 2>/dev/null
echo "status $?"
$SH -i -c 'var=)' </dev/null 2>/dev/null; echo "status $?"
$SH -i -c 'test -o emacs; echo $?' </dev/null 2>/dev/null
$SH +c 'echo plus'
