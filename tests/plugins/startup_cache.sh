# A directory plugin loaded from rc.d: the cache keeps what its rc.lsh did
# and loads its extension.rhai again, without running rc.lsh. Changing either
# file makes the next shell run rc.d again.
mkdir -p .config/luish/rc.d .config/luish/plugins/p d
echo '__luish_internal plugin load p' > .config/luish/rc.d/plugins.lsh
cd .config/luish/plugins/p
cat > extension.rhai <<'X'
print(`rhai runs (${sh::getvar("LUISH_PLUGIN_NAME")})`);
sh::hook("chpwd", |a, b| print("hook v1"));
X
cat > rc.lsh <<'X'
echo "rc.lsh runs"
f() { echo "f v1"; }
X
cd
show='__luish_internal plugin list-loaded; f; cd d'
echo '--- builds the cache'
$SH -i -c "$show" 2>/dev/null
echo '--- uses it'
$SH -i -c "$show" 2>/dev/null
echo '--- rc.lsh changed'
sed -i 's/f v1/f v2/' .config/luish/plugins/p/rc.lsh
$SH -i -c "$show" 2>/dev/null
$SH -i -c "$show" 2>/dev/null
echo '--- extension.rhai changed'
sed -i 's/hook v1/hook v2/' .config/luish/plugins/p/extension.rhai
$SH -i -c "$show" 2>/dev/null
$SH -i -c "$show" 2>/dev/null
