# While variables are traced, the startup files run instead of being
# restored from the cache, so that where they set variables is recorded,
# blocks included.
mkdir -p .config/luish/rc.d
cat > .config/luish/rc.d/10-path.lsh <<'X'
export PATH=/rc/bin:$PATH
EDITOR=vi
X
cat > .config/luish/rc.d/20-prompt.lsh <<'X'
PS1='%~ $ '
__luish_cache env=(HOME) {
  PATH=$PATH:/opt/x
  CACHED=1
}
X
# The first builds the cache, the second uses it.
for i in 1 2; do
  env -i HOME="$HOME" PATH=/usr/bin:/bin $SH -i +m -c 'echo "$PATH $CACHED"' 2>/dev/null
done
env -i HOME="$HOME" PATH=/usr/bin:/bin $SH -i +m -o vars.trace -c 'where EDITOR CACHED PS1; where -a PATH; X=1; where X'
