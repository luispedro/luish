# luish's own options have grouped names (`history.share`), and their
# earlier names and zsh's still work. `setopt NAME=VALUE` sets an option
# on or off, or a setting with a value (the history's variables). Its
# arguments are expanded as assignments: tilde expansion after `=` and
# `:`, and no field splitting.
on() { if setopt | grep -qx "$1"; then echo "$1 on"; else echo "$1 off"; fi; }
setopt history.share; on history.share
unsetopt share_history; on history.share
setopt HISTORY.Share=yes; on history.share
setopt history.share=OFF; on history.share
setopt history.no_share=false; on history.share
unsetopt history.no_share; on history.share
setopt sharehistory=0 glob.star=true; on history.share; on glob.star
setopt no_glob=on; case $- in *f*) echo "noglob on";; esac
setopt glob=true; case $- in *f*) ;; *) echo "noglob off";; esac
setopt history.file=~/hist history.size=50 History.Save_Size=20
[ "$HISTFILE" = "$HOME/hist" ] && echo "tilde expanded"
echo "$HISTSIZE $SAVEHIST"
v='a  b'
setopt history.file=$v; echo "[$HISTFILE]"
setopt history.file=a:~/x; [ "$HISTFILE" = "a:$HOME/x" ] && echo "tilde after :"
setopt 'history.file=~/q'; echo "[$HISTFILE]"
command setopt history.file=~/y; [ "$HISTFILE" = "$HOME/y" ] && echo "through command"
unsetopt history.file history.size; echo "${HISTFILE-unset} ${HISTSIZE-unset}"
setopt history.file=; echo "[${HISTFILE-unset}]"
# Errors: status 1, and the other arguments are still set.
setopt history.size=12x history.share=maybe history.file history.bogus=1 history.nofile glob.star=off 2>&1
echo "status $?"; on glob.star
unsetopt history.share=0 2>&1; echo "status $?"
readonly SAVEHIST
setopt history.save_size=1 2>&1; echo "status $?"
unsetopt history.save_size 2>&1; echo "status $?"; echo "$SAVEHIST"
