# A plugin can be a directory with plugin.rhai (run first) and rc.lsh (run
# next, as with `.`). LUISH_PLUGIN_DIR and LUISH_PLUGIN_NAME are set while
# they run; `import` and sh::plugin_dir() use the plugin's directory.
P=$HOME/.config/luish/plugins
mkdir -p "$P/greet" "$P/shonly" "$P/both" d empty
cd "$P/greet"
cat > plugin.rhai <<'X'
import "util" as u;
print(`rhai: ${u::hello()} ${sh::getvar("LUISH_PLUGIN_NAME")}`);
sh::setvar("FROM_RHAI", "set by plugin.rhai");
let dir = sh::plugin_dir();
sh::hook("chpwd", |a, b| print(`hook: dir ${dir == sh::plugin_dir()}`));
X
echo 'fn hello() { "hello from util" }' > util.rhai
cat > rc.lsh <<'X'
echo "rc: $FROM_RHAI, name $LUISH_PLUGIN_NAME"
. "$LUISH_PLUGIN_DIR/lib.lsh"
greet_dir=$LUISH_PLUGIN_DIR
X
echo 'greet() { echo "greet from lib"; }' > lib.lsh
echo 'alias sh_only="echo shell only"' > "$P/shonly/rc.lsh"
echo 'print("both: the file wins");' > "$P/both.rhai"
echo 'print("both: the directory");' > "$P/both/plugin.rhai"
touch "$P/.hidden.rhai" "$P/README"
cd
main() {
echo "--- the plugins in the plugin directory"
__luish_internal plugin list-available
LUISH_PLUGIN_NAME=before
__luish_internal plugin load greet shonly both
echo "load: $?"
echo "after: DIR=${LUISH_PLUGIN_DIR-unset} NAME=$LUISH_PLUGIN_NAME"
[ "$greet_dir" = "$P/greet" ] && echo "LUISH_PLUGIN_DIR is absolute"
greet
eval sh_only
__luish_internal plugin list-loaded
cd d; cd ..
echo "--- a path to a directory, with a trailing slash"
__luish_internal plugin load ./.config/luish/plugins/shonly/
__luish_internal plugin list-loaded
echo "--- a directory without an entry point"
__luish_internal plugin load ./empty
echo "status $?"
echo "--- a failing plugin.rhai: rc.lsh doesn't run"
mkdir bad
echo 'throw "no";' > bad/plugin.rhai
echo 'echo "not reached"' > bad/rc.lsh
__luish_internal plugin load ./bad
echo "status $?"
echo "--- a missing module"
mkdir nomod
echo 'import "nosuch" as n;' > nomod/plugin.rhai
__luish_internal plugin load ./nomod
echo "status $?"
echo "--- loading again reads the modules again"
echo 'fn hello() { "hello again" }' > "$P/greet/util.rhai"
__luish_internal plugin load greet
echo "--- unloading a shell-only plugin"
__luish_internal plugin unload shonly
__luish_internal plugin list-loaded
}
main | sed "s|$HOME|HOME|g"
