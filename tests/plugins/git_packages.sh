# Git sources in config.toml: `plugin sync` fetches them and pins each to a
# commit in plugins.lock; startup and `plugin load` use the pinned commits,
# never git. `plugin sync` doesn't move a pinned source; `plugin update`
# does. Plugins that aren't installed are reported, and the startup cache
# isn't written until they are.
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
C=$HOME/.config/luish
mkdir -p "$C" luish/std/hello coll/plugins/x single
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
cmd() { $SH -c "$1" 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"; }
commit() { (cd "$1" && git add -A && git commit -qm "$2"); }
# `std`, overridden here: a collection in a subdirectory.
echo 'echo "hello from std"' > luish/std/hello/init.lsh
echo 'print("rhai from std");' > luish/std/rhai-only.rhai
(cd luish && git init -q -b main) && commit luish one
# A collection whose plugin x needs std/hello.
echo 'echo "x (needs hello)"' > coll/plugins/x/init.lsh
printf '[dependencies]\n"std/hello" = "*"\n' > coll/plugins/x/plugin.toml
(cd coll && git init -q -b main) && commit coll one
# A repository that is one plugin.
echo 'echo "single v1"' > single/init.lsh
(cd single && git init -q -b main) && commit single one
cat > "$C/config.toml" <<X
[plugins.available]
std = { git = "file://$HOME/luish", subdir = "std" }
coll = { git = "file://$HOME/coll", subdir = "plugins/" }

[plugins.enabled]
std.rhai-only = "*"
single = { git = "file://$HOME/single", branch = "main" }
X
echo '--- not installed: reported by every shell'
run '__luish_internal plugin list-loaded'
run '__luish_internal plugin list-loaded'
echo '--- plugin sync'
cmd '__luish_internal plugin sync; echo "status $?"'
sed "s|$HOME|~|g" "$C/plugins.lock"
echo '--- startup'
run '__luish_internal plugin list-loaded'
run '__luish_internal plugin list-loaded'
echo '--- the plugins of the available sources can be loaded'
cmd '__luish_internal plugin list-available'
cmd '__luish_internal plugin load coll/x; __luish_internal plugin list-loaded'
echo '--- plugin sync does not move a pinned source'
echo 'echo "single v2"' > single/init.lsh
commit single two
cmd '__luish_internal plugin sync; echo "status $?"'
run 'true'
echo '--- plugin update does'
cmd '__luish_internal plugin update single; echo "status $?"'
run 'true'
cmd '__luish_internal plugin update; echo "status $?"'
cmd '__luish_internal plugin update nosuch; echo "status $?"'
echo '--- a removed data directory: not installed, and synced again at the same commit'
rm -rf .local/share/luish
run 'true'
echo 'echo "single v3"' > single/init.lsh
commit single three
cmd '__luish_internal plugin sync; echo "status $?"'
run 'true'
echo '--- a removed cache: fetched again, loading is unaffected'
rm -rf .cache/luish/plugins
run 'true'
cmd '__luish_internal plugin update single; echo "status $?"'
ls .cache/luish/plugins/git | sed 's/-.*//'
cat .local/share/luish/plugins/README | head -1
echo '--- a tag, and a commit'
first=$(cd single && git rev-list --max-parents=0 HEAD)
(cd single && git tag v1 "$first")
cat > "$C/config.toml" <<X
[plugins.enabled]
tagged = { git = "file://$HOME/single", tag = "v1" }
X
cmd '__luish_internal plugin sync; echo "status $?"'
run 'true'
cat > "$C/config.toml" <<X
[plugins.enabled]
pinned = { git = "file://$HOME/single", rev = "$first" }
X
cmd '__luish_internal plugin sync; echo "status $?"'
run 'true'
grep -c '^\[\[source\]\]' "$C/plugins.lock"
echo '--- a repository that cannot be fetched'
cat > "$C/config.toml" <<X
[plugins.enabled]
gone = { git = "file://$HOME/nosuch" }
X
# (git's own messages vary with its version.)
$SH -c '__luish_internal plugin sync; echo "status $?"' 2>&1 | grep -e "^$SH" -e status | sed "s|$SH|luish|; s|$HOME|~|g"
echo '--- nothing to lock: no lock file'
rm "$C/plugins.lock" "$C/config.toml"
cmd '__luish_internal plugin sync; echo "status $?"'
ls "$C"
echo '--- a lock from a newer luish is left alone'
printf 'version = 2\n' > "$C/plugins.lock"
cmd '__luish_internal plugin sync; echo "status $?"'
cat "$C/plugins.lock"
