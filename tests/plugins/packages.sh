# config.toml's [plugins] table: plugins.enabled lists the plugins that
# interactive shells load at startup (after rc.d, cached with it), as NAME (a
# source in plugins.available, else a plugin in the plugin directory),
# SOURCE/NAME or SOURCE.NAME (a plugin of a collection, loaded as
# SOURCE/NAME), or a source of its own. (tests/plugins/nested.sh has
# sub-collections.) A directory plugin's plugin.toml lists its dependencies, which load
# first; a plain NAME there is in the same collection. Local plugins need no
# lock. (tests/plugins/git_packages.sh has git sources.)
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$P/greet" "$HOME/src/coll/tool" "$HOME/src/coll/cyc1" "$HOME/src/coll/cyc2" "$HOME/src/one"
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
# In the plugin directory: greet needs helper, next to it, and the tool of
# the collection `coll`.
echo 'echo "greet (helper: $helper, tool: $tool)"' > "$P/greet/init.lsh"
cat > "$P/greet/plugin.toml" <<'X'
description = "Keys other than dependencies are ignored."
[dependencies]
helper = "*"
"coll/tool" = "*"
X
echo 'echo helper; helper=yes' > "$P/helper.lsh"
# A collection, with a plugin that needs one of its own and a source of its
# own (a relative path is relative to the plugin).
echo 'echo "tool (lib: $lib, one: $one)"; tool=yes' > "$HOME/src/coll/tool/init.lsh"
cat > "$HOME/src/coll/tool/plugin.toml" <<'X'
[dependencies]
lib = "*"
one = { path = "../../one" }
X
echo 'echo lib; lib=yes' > "$HOME/src/coll/lib.lsh"
echo 'print("one"); sh::setvar("one", "yes");' > "$HOME/src/one/extension.rhai"
printf '[dependencies]\ncyc2 = "*"\n' > "$HOME/src/coll/cyc1/plugin.toml"
echo 'echo cyc1' > "$HOME/src/coll/cyc1/init.lsh"
printf '[dependencies]\ncyc1 = "*"\n' > "$HOME/src/coll/cyc2/plugin.toml"
echo 'echo cyc2' > "$HOME/src/coll/cyc2/init.lsh"
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }

[plugins.enabled]
greet = "*"
coll.lib = "*"      # already loaded, as greet's dependency: loaded once
X
echo '--- startup: dependencies first, each once'
run '__luish_internal plugin list-loaded'
echo '--- a later shell uses the cache (the extension is loaded again)'
run '__luish_internal plugin list-loaded'
echo '--- changing a manifest is noticed'
printf '[dependencies]\nhelper = "*"\n' > "$P/greet/plugin.toml"
run '__luish_internal plugin list-loaded'
echo '--- scripts and --no-plugins load nothing'
$SH -c '__luish_internal plugin list-loaded'
$SH -i --no-plugins -c '__luish_internal plugin list-loaded' 2>/dev/null
echo '--- plugin list-available adds the plugins of the sources'
$SH -c '__luish_internal plugin list-available'
echo '--- plugin load: by SOURCE/NAME, with the dependencies not loaded yet'
$SH -c '__luish_internal plugin load helper coll/tool; __luish_internal plugin list-loaded'
echo '--- plugin list-available leaves out the loaded plugins, SOURCE/NAME too'
$SH -c '__luish_internal plugin load helper coll/tool >/dev/null; __luish_internal plugin list-available'
echo '--- plugin unload SOURCE/NAME, and a path'
$SH -c 'u() { __luish_internal plugin unload "$@"; }
__luish_internal plugin load helper coll/tool >/dev/null; u coll/tool ./src/coll/lib.lsh
__luish_internal plugin list-loaded; u coll/tool; u coll/nosuch nosuch/x; echo "status $?"' 2>&1 | sed "s|$SH|luish|"
echo '--- plugin load: a path, whose plain dependencies are next to it'
$SH -c '__luish_internal plugin load ./src/coll/tool; __luish_internal plugin list-loaded'
echo '--- a dependency cycle'
$SH -c '__luish_internal plugin load coll/cyc1; echo "status $?"' 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"
echo '--- a source that is one plugin, and a plugin of a source of its own'
cat > "$C/config.toml" <<'X'
[plugins.available]
one = { path = "~/src/one" }
[plugins.enabled]
one = "*"
t = { path = "~/src/coll", plugin = "tool" }
X
run '__luish_internal plugin list-loaded'
echo '--- errors'
cat > "$C/config.toml" <<'X'
[plugins]
bogus = 1
[plugins.available]
std = "luispedro/luish"
x = { gh = "no-slash" }
y = { git = "u", path = "p" }
z = { path = "p", branch = "main" }
w = { gh = "a/b", rev = "abc" }
v = { gh = "a/b", subdir = "../x" }
u = { gh = "a/b", colour = "blue" }
[plugins.enabled]
greet = "1.0"
"a/b/c" = "*"
"a/b//c" = "*"
"/x" = "*"
nosuch = "*"
nosrc.x = "*"
helper = "*"
X
run '__luish_internal plugin list-loaded'
echo '--- SOURCE/NAME must be quoted in TOML'
printf '[plugins.enabled]\ncoll/lib = "*"\n' > "$C/config.toml"
run '__luish_internal plugin list-loaded'
echo '--- plugins of a collection are named SOURCE/NAME, so these differ'
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }
[plugins.enabled]
helper = "*"
coll.helper = "*"
X
echo 'echo other helper' > "$HOME/src/coll/helper.lsh"
run '__luish_internal plugin list-loaded'
echo '--- the same name for two plugins: those of a source without a name are'
echo '--- named after their files'
cat > "$C/config.toml" <<'X'
[plugins.enabled]
lib = "*"
t = { path = "~/src/coll", plugin = "tool" }
X
echo 'echo local lib' > "$P/lib.lsh"
run '__luish_internal plugin list-loaded'
