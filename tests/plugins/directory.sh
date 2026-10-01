# A plugin can be a directory with init.lsh (run first, as with `.`),
# extension.rhai (next) and rc.lsh (last, only in interactive shells); a
# NAME.rhai file (just an extension); or a NAME.lsh file (just an init.lsh).
# LUISH_PLUGIN_DIR and LUISH_PLUGIN_NAME are set while they run; `import` and
# sh::plugin_dir() use the plugin's directory.
P=$HOME/.config/luish/plugins
mkdir -p "$P/greet" "$P/shonly" "$P/both" "$P/lshdir" "$P/notes" d empty
cd "$P/greet"
cat > init.lsh <<'X'
echo "init: name $LUISH_PLUGIN_NAME"
. "$LUISH_PLUGIN_DIR/lib.lsh"
greet_dir=$LUISH_PLUGIN_DIR
FROM_INIT="set by init.lsh"
X
cat > extension.rhai <<'X'
import "util" as u;
print(`rhai: ${u::hello()} ${sh::getvar("LUISH_PLUGIN_NAME")}, ${sh::getvar("FROM_INIT")}`);
sh::setvar("FROM_RHAI", "set by extension.rhai");
let dir = sh::plugin_dir();
sh::hook("chpwd", |a, b| print(`hook: dir ${dir == sh::plugin_dir()}`));
X
echo 'fn hello() { "hello from util" }' > util.rhai
echo 'echo "rc: $FROM_RHAI"' > rc.lsh
echo 'greet() { echo "greet from lib"; }' > lib.lsh
echo 'alias sh_only="echo shell only"' > "$P/shonly/init.lsh"
echo 'print("both: the .rhai file wins");' > "$P/both.rhai"
echo 'echo "both: the .lsh file"' > "$P/both.lsh"
echo 'print("both: the directory");' > "$P/both/extension.rhai"
cat > "$P/single.lsh" <<'X'
echo "single: $LUISH_PLUGIN_NAME in $LUISH_PLUGIN_DIR"
single() { echo "single from single.lsh"; }
X
echo 'echo "lshdir: the .lsh file wins"' > "$P/lshdir.lsh"
echo 'echo "lshdir: the directory"' > "$P/lshdir/init.lsh"
touch "$P/.hidden.rhai" "$P/.hidden.lsh" "$P/README" "$P/notes/todo.txt"
cd
main() {
echo "--- the plugins in the plugin directory"
__luish_internal plugin list-available
LUISH_PLUGIN_NAME=before
__luish_internal plugin load greet shonly both single lshdir
echo "load: $?"
echo "after: DIR=${LUISH_PLUGIN_DIR-unset} NAME=$LUISH_PLUGIN_NAME"
[ "$greet_dir" = "$P/greet" ] && echo "LUISH_PLUGIN_DIR is absolute"
greet
eval sh_only
single
__luish_internal plugin list-loaded
cd d; cd ..
echo "--- a path to a directory, with a trailing slash"
__luish_internal plugin load ./.config/luish/plugins/shonly/
__luish_internal plugin list-loaded
echo "--- a path to a .lsh file"
echo 'echo "other: $LUISH_PLUGIN_NAME"' > other.lsh
__luish_internal plugin load ./other.lsh
__luish_internal plugin load ./missing.lsh
echo "status $?"
echo "--- a directory without an entry point"
__luish_internal plugin load ./empty
echo "status $?"
echo "--- a failing extension.rhai, after init.lsh"
mkdir bad
echo 'echo "init runs"' > bad/init.lsh
echo 'throw "no";' > bad/extension.rhai
echo 'echo "not reached"' > bad/rc.lsh
__luish_internal plugin load ./bad
echo "status $?"
echo "--- a missing module"
mkdir nomod
echo 'import "nosuch" as n;' > nomod/extension.rhai
__luish_internal plugin load ./nomod
echo "status $?"
echo "--- loading again reads the modules again"
echo 'fn hello() { "hello again" }' > "$P/greet/util.rhai"
__luish_internal plugin load greet
echo "--- unloading shell-only plugins"
__luish_internal plugin unload shonly single
__luish_internal plugin list-loaded
echo "--- an interactive shell runs rc.lsh, unless extension.rhai fails"
$SH -i -c '__luish_internal plugin load greet ./bad' 2>/dev/null
}
main | sed "s|$HOME|HOME|g"
