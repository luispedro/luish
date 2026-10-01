# A library is a plugin whose plugin.toml says `library = true`: one for
# other plugins to use (as a dependency, with `import "@SOURCE/PLUGIN/MODULE"`
# or the shell functions it defines). It loads as any other plugin, but
# `plugin list-available` leaves it out unless given -a (or --all), and it
# doesn't count when a collection's only plugin is chosen. A directory whose
# only entry point is plugin.toml is a plugin.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$P/rlib" "$P/shlib" "$P/user" "$P/both" "$P/meta" "$HOME/src/coll/tool" "$HOME/src/coll/clib"
# A library of Rhai modules only.
printf 'description = "Helpers"\nlibrary = true\n' > "$P/rlib/plugin.toml"
echo 'fn greet(who) { `hello, ${who}` }' > "$P/rlib/util.rhai"
# A library of shell functions.
echo 'library = true' > "$P/shlib/plugin.toml"
echo 'shout() { echo "$*!"; }' > "$P/shlib/init.lsh"
printf '[dependencies]\nrlib = "*"\nshlib = "*"\n' > "$P/user/plugin.toml"
cat > "$P/user/extension.rhai" <<'X'
import "@local/rlib/util" as util;
print(util::greet("user"));
X
# NAME.rhai comes before NAME/, so `both` is no library.
echo 'library = true' > "$P/both/plugin.toml"
echo 'print("both.rhai");' > "$P/both.rhai"
# Only a manifest: a plugin that pulls in others.
printf '[dependencies]\nshlib = "*"\n' > "$P/meta/plugin.toml"
# A collection of one plugin and a library.
echo 'echo tool' > "$HOME/src/coll/tool/init.lsh"
printf 'library = true\n[dependencies]\n' > "$HOME/src/coll/clib/plugin.toml"
printf '[plugins.available]\ncoll = { path = "~/src/coll" }\n' > "$C/config.toml"
p() { __luish_internal plugin "$@"; }
echo '--- list-available leaves out the libraries'
p list-available
echo '--- unless given -a or --all'
p list-available -a
p list-available --all | tr '\n' ' '; echo
echo '--- a bad argument'
p list-available -x 2>/dev/null
echo "status $?"
echo '--- loading a plugin loads the libraries it needs'
p load user
p list-loaded
shout hi
echo '--- the loaded libraries are not available any more, even with -a'
p list-available -a | grep -c -e rlib -e shlib
echo '--- a library can be loaded by name'
$SH -c '__luish_internal plugin load shlib && shout direct'
echo '--- a directory with only plugin.toml is a plugin'
$SH -c '__luish_internal plugin load meta; __luish_internal plugin list-loaded'
echo '--- the only plugin of a collection is the one that is no library'
printf '[plugins.available]\nc = { path = "~/src/coll" }\n[plugins.enabled]\nc = "*"\n' > "$C/config.toml"
$SH -i -c '__luish_internal plugin list-loaded' 2>&1 | grep -v 'job control'
echo '--- library must be a boolean (reported as other errors in plugin.toml)'
printf 'library = "yes"\n' > "$P/shlib/plugin.toml"
$SH -c '__luish_internal plugin load user' 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"
echo "status $?"
