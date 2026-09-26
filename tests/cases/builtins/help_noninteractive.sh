# `help` is a built-in only in interactive shells, so a script finds the
# same commands as in dash.
help
echo "status $?"
help cd
echo "status $?"
type help
command -v help || echo "status $?"
