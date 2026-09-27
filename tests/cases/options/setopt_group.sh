# `setopt -p GROUP` and `unsetopt -p GROUP`: the names that follow are in
# GROUP (so `share` is `history.share`, and `no_share` is
# `history.no_share`). Without names, they print the group's settings as
# the commands that set them.
on() { if setopt | grep -qx "$1"; then echo "$1 on"; else echo "$1 off"; fi; }
setopt -p history share ignore_space; on history.share; on history.ignore_space
unsetopt -p History share; on history.share
setopt -p history no_ignore_space; on history.ignore_space
setopt -p history reduce_blanks=yes save_no_dups=false; on history.reduce_blanks
unsetopt -p history share; setopt -phistory -- share; on history.share
setopt -p glob -p history inc_append; on history.inc_append
# The arguments are still expanded as assignments.
setopt -p history file=~/hist size=100; [ "$HISTFILE" = "$HOME/hist" ] && echo "tilde expanded"
command setopt -p history file=~/h2; [ "$HISTFILE" = "$HOME/h2" ] && echo "through command"
echo "$HISTSIZE"
# Listing: values only if their variable is set, quoted to be read back.
HISTFILE="it's"
setopt -p history
unsetopt -p history size file
unsetopt -p HISTORY
setopt -p glob
# Errors.
setopt -p history bogus errexit share=maybe 2>&1; echo "status $?"
setopt -p bogus share 2>&1; echo "status $?"
setopt -p history.share 2>&1; echo "status $?"
setopt -p 2>&1; echo "status $?"
setopt -x 2>&1; echo "status $?"
setopt -- -p 2>&1; echo "status $?"
