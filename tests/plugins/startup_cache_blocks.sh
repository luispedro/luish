# A `__luish_cache` block in the `rc.lsh` of a plugin loaded with
# `plugin load` while `luishrc` runs is cached with luishrc's own blocks,
# in the `startup` cache (not `rc.d`'s: the plugin isn't loaded from
# `config.toml`).
P=$HOME/.config/luish/plugins
mkdir -p "$P/p" .config/luish
echo 'echo running p/rc.lsh; __luish_cache env=(TOOL) { echo "p block for $TOOL"; P_VAR=$TOOL; }' > "$P/p/rc.lsh"
echo 'plugin load p' > .config/luish/luishrc
show() {
    TOOL=$1 $SH -i -c 'echo "P_VAR=$P_VAR"' 2>&1 | grep -v 'job control'
}
show a
show a
echo '--- a different TOOL rebuilds the block'
show b
