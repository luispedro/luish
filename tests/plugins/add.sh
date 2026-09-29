# `plugin add PLUGIN [NAME]` adds a plugin to config.toml, after asking
# (unless -y), then runs `plugin sync` and loads it. PLUGIN is a GitHub
# repository or URL, another git URL, a local path (or a file:// URL that
# isn't a repository), or a plugin of a source that config.toml names. A git
# source is fetched first (into a temporary directory), to see what it
# holds. The file keeps its comments; a collection of more than one plugin
# goes to plugins.available. (The parsing of GitHub URLs is tested in
# src/plugins/add.rs.)
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
C=$HOME/.config/luish
mkdir -p "$C/plugins" luish/std/hello coll/a coll/b single gcoll/p gcoll/q
cmd() { $SH -c "$1" 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"; }
echo 'echo "hello from std"' > luish/std/hello/init.lsh
(cd luish && git init -q -b main && git add -A && git commit -qm one)
echo 'echo "single loaded"' > single/init.lsh
(cd single && git init -q -b main && git add -A && git commit -qm one)
echo 'echo p' > gcoll/p/init.lsh
echo 'echo q' > gcoll/q/init.lsh
(cd gcoll && git init -q -b main && git add -A && git commit -qm one)
echo 'echo a' > coll/a/init.lsh
echo 'echo b' > coll/b/init.lsh
echo 'echo "mine loaded"' > "$C/plugins/mine.lsh"
cat > "$C/config.toml" <<X
# My configuration.
[plugins.available]
std = { git = "file://$HOME/luish", subdir = "std" }

[plugins.enabled]
# Nothing yet.

[options]
X
echo '--- declined: nothing changes'
cp "$C/config.toml" before
echo n | cmd '__luish_internal plugin add file://$HOME/single; echo "status $?"'
echo | cmd '__luish_internal plugin add file://$HOME/single; echo "status $?"'
cmd '__luish_internal plugin add file://$HOME/single </dev/null; echo "status $?"'
cmp before "$C/config.toml" && echo same
echo '--- a git URL, accepted'
echo y | cmd '__luish_internal plugin add file://$HOME/single; echo "status $?"'
echo '--- a git collection goes to plugins.available'
cmd '__luish_internal plugin add -y file://$HOME/gcoll; echo "status $?"'
cmd '__luish_internal plugin load gcoll/q'
ls -A ~/.local/share/luish/plugins
echo '--- a git repository that is not there is not added'
cmd '__luish_internal plugin add -y file://$HOME/nothing.git/; echo "status $?"'
mkdir -p nothing.git/objects && touch nothing.git/HEAD
$SH -c '__luish_internal plugin add -y file://$HOME/nothing.git/' >/dev/null 2>&1; echo "status $?"
grep -c nothing "$C/config.toml"
echo '--- a plugin of a named source, and one of the plugin directory'
cmd '__luish_internal plugin add -y std/hello; echo "status $?"'
cmd '__luish_internal plugin add -y mine; echo "status $?"'
echo '--- a local path, with a name; a local collection'
cmd '__luish_internal plugin add --yes ./coll/a first; echo "status $?"'
cmd '__luish_internal plugin add -y ~/coll; echo "status $?"'
cmd '__luish_internal plugin add -y file://$HOME/coll/b; echo "status $?"'
sed "s|$HOME|~|g" "$C/config.toml"
echo '--- startup loads them'
$SH -i -c 'true' 2>&1 | grep -v 'job control'
echo '--- errors'
cmd '__luish_internal plugin add -y ./coll/a first; echo "status $?"'
cmd '__luish_internal plugin add -y nothing; echo "status $?"'
cmd '__luish_internal plugin add -y https://github.com/o/r/blob/main/x; echo "status $?"'
cmd '__luish_internal plugin add -y ./nothing; echo "status $?"'
cmd '__luish_internal plugin add; echo "status $?"'
echo '[plugins]' > "$C/config.toml"; echo 'enabled = {}' >> "$C/config.toml"
cmd '__luish_internal plugin add -y ./coll/a; echo "status $?"'
echo '--- a configuration that is a symbolic link stays one'
mv "$C/config.toml" dotfiles.toml && ln -s ../../dotfiles.toml "$C/config.toml"
echo '[plugins.enabled]' > dotfiles.toml
cmd '__luish_internal plugin add -y ./coll/b >/dev/null; echo "status $?"'
[ -L "$C/config.toml" ] && sed "s|$HOME|~|g" dotfiles.toml
