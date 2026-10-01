# reference: zsh
# zsh's special parameters RANDOM, SECONDS, UID, EUID, GID and EGID. RANDOM
# is the C library's rand(), seeded by assigning to it.
RANDOM=1
echo $RANDOM $RANDOM
(echo $RANDOM)
echo $((RANDOM >= 0 && RANDOM < 32768))
echo ${RANDOM+set} ${#RANDOM}
RANDOM=42
a=$RANDOM
RANDOM=42
[ "$a" = "$RANDOM" ] && echo same
# Unset, they read as unset until assigned again.
unset RANDOM
echo "[${RANDOM-unset}]"
RANDOM=3
echo $RANDOM
SECONDS=100
echo $SECONDS
: $((SECONDS = 20))
echo $SECONDS
unset SECONDS
echo "[${SECONDS-unset}]"
SECONDS=5
echo $SECONDS
[ "$UID" = "$(id -u)" ] && echo uid
[ "$EUID" = "$(id -u)" ] && echo euid
[ "$GID" = "$(id -g)" ] && echo gid
[ "$EGID" = "$(id -g)" ] && echo egid
echo $((UID == $(id -u)))
unset UID
echo "[${UID-unset}]"
# MACHTYPE and OSTYPE, which the environment doesn't set.
case $OSTYPE in linux-*) echo $MACHTYPE linux;; *) echo $OSTYPE;; esac
MACHTYPE=x OSTYPE=y $SH -c 'echo $MACHTYPE $OSTYPE' | grep -c y
OSTYPE=z
echo $OSTYPE
unset MACHTYPE
echo "[${MACHTYPE-unset}]"
