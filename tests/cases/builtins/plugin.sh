# `plugin` is a built-in only in interactive shells, so a script finds the
# same commands as in dash.
plugin list
echo "status $?"
type plugin
command -v plugin || echo "status $?"
