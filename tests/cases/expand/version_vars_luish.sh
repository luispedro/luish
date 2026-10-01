# LUISH_VERSION, LUISH_PATCHLEVEL and HOSTTYPE (zsh's ZSH_VERSION and
# ZSH_PATCHLEVEL, and bash's HOSTTYPE): set but not exported, so that a
# child shell can tell it isn't luish; the environment doesn't set them.
[ "luish $LUISH_VERSION" = "$($SH --version | cut -d' ' -f1-2)" ] && echo version
case $LUISH_PATCHLEVEL in unknown|[0-9a-f]*[0-9a-f]|[0-9a-f]*-dirty) echo patchlevel;; *) echo bad;; esac
[ "$HOSTTYPE" = "$(uname -m)" ] && echo hosttype
[ "$HOSTTYPE" = "$MACHTYPE" ] && echo machtype
env | grep -E '^(LUISH_VERSION|LUISH_PATCHLEVEL|MACHTYPE|HOSTTYPE|OSTYPE)=' || echo not exported
LUISH_VERSION=x LUISH_PATCHLEVEL=y HOSTTYPE=z $SH -c '[ "$LUISH_VERSION" != x ] && [ "$LUISH_PATCHLEVEL" != y ] &&
[ "$HOSTTYPE" != z ] && echo ignored'
# Assigning makes one an ordinary variable; unset, it reads as unset.
LUISH_VERSION=1.0
echo $LUISH_VERSION
unset HOSTTYPE
echo "[${HOSTTYPE-unset}]"
