# `vars.trace_after_startup`: the startup files run as usual (from the
# cache, when they can), and tracing starts after them, as if
# `setopt vars.trace` were the first command: what they set shows as set
# during startup.
mkdir -p .config/luish/rc.d
cat > .config/luish/rc.d/10-path.lsh <<'X'
export PATH=/rc/bin:$PATH
EDITOR=vi
X
cat > .config/luish/rc.d/20-cache.lsh <<'X'
__luish_cache env=(HOME) {
  echo built >&2
  CACHED=1
}
X
run() {
  env -i HOME="$HOME" PATH=/usr/bin:/bin $SH -i +m "$@" 2>&1
}
# Tracing doesn't start in the startup files, so the cache is used.
run -c 'echo $CACHED'
run -o vars.trace_after_startup -c 'where EDITOR CACHED PATH; X=1; where X'
# Set in the startup files too.
echo 'setopt vars.trace_after_startup' > .config/luish/rc.d/05-trace.lsh
run -c 'where EDITOR; where -a PATH; unsetopt vars.trace_after_startup; where EDITOR 2>/dev/null || echo off'
# With vars.trace_history too, all changes are kept, from then on.
run -o vars.trace_history -c 'Y=1; Y=2; where -a Y EDITOR'
# Not in a script (no startup files): tracing starts at once.
printf 'Z=1\nwhere Z\n' > s.sh
$SH -o vars.trace_after_startup s.sh
