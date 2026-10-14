# Without startup.auto_cache, the plugins that config.toml enables load in
# every shell, and their `__luish_cache` blocks are cached as entries of
# their own (keyed on their env=(...) and files=(...)), not nested in the
# entry of config.toml, which isn't saved. Their post-rc.lsh files run every
# time too.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$C/rc.d" "$P/p"
cat > "$P/p/init.lsh" <<'X'
echo "p: init.lsh runs"
__luish_cache env=(TOOL) files=("$HOME/dep") {
    echo "p: block built for $TOOL"
    P_VAR=$TOOL$(cat "$HOME/dep" 2>/dev/null)
}
X
echo 'echo "p: post-rc.lsh runs"' > "$P/p/post-rc.lsh"
printf '[plugins.enabled]\np = "*"\n' > "$C/config.toml"
echo 'echo "rc.d: P_VAR=$P_VAR"' > "$C/rc.d/a.lsh"
show() {
    $SH -i -c 'echo "P_VAR=$P_VAR"' 2>&1 | grep -v 'job control'
}
echo '--- built, then restored'
TOOL=a show
TOOL=a show
echo '--- its variable and its file rebuild it'
TOOL=b show
echo 1 > dep
TOOL=b show
TOOL=b show
echo '--- the cache has the block only'
grep -c '^block ' .cache/luish/rc-*
grep -c '^config$\|^post-rc$\|^file ' .cache/luish/rc-* || true
