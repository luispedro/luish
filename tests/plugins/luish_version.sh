# A plugin's plugin.toml can say the oldest luish it needs, as
# `luish-version = "0.5"`. An older luish doesn't load it (nor the plugins
# that depend on it) and says why; a value that isn't a version is reported
# as other errors in plugin.toml, and the plugin loads.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$P/old" "$P/new" "$P/user" "$P/bad"
printf 'luish-version = "0.1"\n' > "$P/old/plugin.toml"
echo 'echo old loaded' > "$P/old/init.lsh"
printf 'luish-version = "999.0"\n[dependencies]\nmissing = "*"\n' > "$P/new/plugin.toml"
echo 'echo new loaded' > "$P/new/init.lsh"
printf '[dependencies]\nold = "*"\nnew = "*"\n' > "$P/user/plugin.toml"
echo 'echo user loaded' > "$P/user/init.lsh"
printf 'luish-version = ">=0.5"\n' > "$P/bad/plugin.toml"
echo 'echo bad loaded' > "$P/bad/init.lsh"
p() { __luish_internal plugin "$@"; }
v() { sed "s|$SH|luish|; s|$HOME|~|g; s|(this is $LUISH_VERSION)|(this is VERSION)|"; }
echo '--- an older version is fine'
p load old
echo "status $?"
echo '--- so is this version'
printf 'luish-version = "%s"\n' "$LUISH_VERSION" > "$P/old/plugin.toml"
$SH -c '__luish_internal plugin load old'
echo "status $?"
echo '--- a newer one is not loaded, and its dependencies are not looked for'
{ $SH -c '__luish_internal plugin load new' 2>&1; echo "status $?"; } | v
echo '--- nor is a plugin that depends on it'
$SH -c '__luish_internal plugin load user; __luish_internal plugin list-loaded' 2>&1 | v
echo '--- nor in plugins.enabled'
printf '[plugins.enabled]\nnew = "*"\nold = "*"\n' > "$C/config.toml"
$SH -i -c '__luish_internal plugin list-loaded' 2>&1 | grep -v 'job control' | v
rm "$C/config.toml"
echo '--- not a version'
$SH -c '__luish_internal plugin load bad' 2>&1 | v
for x in '1' '"0.5.x"' '"1.2.3.4"' '""'; do
	printf 'luish-version = %s\n' "$x" > "$P/bad/plugin.toml"
	$SH -c '__luish_internal plugin load bad' 2>&1 | v | grep -v 'bad loaded'
done
