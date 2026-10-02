# BASH_SOURCE in startup files ($ENV and the cached files of rc.d). The
# functions of a cached file keep their file when the cache restores them.
umask 022
mkdir -p .config/luish/rc.d
cat > .config/luish/rc.d/a.lsh <<'X'
echo "rc: ${BASH_SOURCE#"$HOME"/}"
rcf() { echo "rcf: ${BASH_SOURCE#"$HOME"/}"; }
X
echo 'echo "env: $BASH_SOURCE"; envf() { echo "envf: $BASH_SOURCE"; }' > envfile
echo '--- building the cache'
ENV=./envfile $SH -i -c 'rcf; envf' 2>/dev/null
echo '--- from the cache'
ENV=./envfile $SH -i -c 'rcf; envf; echo "c: ${BASH_SOURCE-unset}"' 2>/dev/null
