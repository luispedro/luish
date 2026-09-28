# luish's specials where zsh differs, or needs zsh/datetime: assigning to UID
# and the others makes them ordinary variables (zsh calls setuid or refuses);
# EPOCHREALTIME has microseconds, as in bash; HISTCMD is 0 without a history;
# a subshell seeds RANDOM anew unless it was assigned; the environment
# doesn't set them.
case $EPOCHSECONDS in *[!0-9]*|'') echo bad;; *) echo epochseconds;; esac
case $EPOCHREALTIME in *.[0-9][0-9][0-9][0-9][0-9][0-9]) echo epochrealtime;; *) echo bad;; esac
[ $((EPOCHSECONDS - ${EPOCHREALTIME%.*})) -le 1 ] && echo close
echo $HISTCMD
UID=abc EUID=x
echo $UID $EUID
unset UID
echo "[${UID-unset}]"
UID=5
echo $UID
EPOCHSECONDS=3
echo $EPOCHSECONDS
a=$( (echo $RANDOM; echo $RANDOM) )
b=$( (echo $RANDOM; echo $RANDOM) )
[ "$a" != "$b" ] && echo differ
RANDOM=7
a=$(echo $RANDOM)
b=$(echo $RANDOM)
[ "$a" = "$b" ] && echo same
RANDOM=9 SECONDS=10 UID=11 $SH -c '[ $SECONDS -lt 10 ] && [ $UID != 11 ] && echo ignored
env | grep -E "^(RANDOM|SECONDS|UID)="'
readonly SECONDS
(SECONDS=3) 2>/dev/null || echo readonly
# A special made ordinary by `local` or before a command is special again
# afterwards.
f() { local GID=x; echo $GID; }
f
GID=y true
case $GID in *[!0-9]*|'') echo bad;; *) echo gid;; esac
