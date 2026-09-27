# SHLVL is incremented where it is set, as in zsh, but a shell that isn't
# interactive doesn't set it (so that scripts see the same environment as
# under dash).
echo "[${SHLVL-unset}]"
SHLVL=3 $SH -c 'echo $SHLVL; env | grep "^SHLVL="'
SHLVL=x $SH -c 'echo $SHLVL'
SHLVL=3 $SH -c '$SH -c "echo \$SHLVL"'
