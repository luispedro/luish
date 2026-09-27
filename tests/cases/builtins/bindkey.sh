# `bindkey` is a built-in only in interactive shells, so a script finds the
# same commands as in dash.
bindkey '^W' 2>/dev/null
echo "status $?"
command -v bindkey || echo "status $?"
