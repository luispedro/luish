# A collection can have sub-collections (directories that aren't plugins),
# to any depth: its plugins are SOURCE/PATH, such as nest/complete/bio, which
# is also their name once loaded. In TOML, SOURCE.SUB.NAME = "*" or
# "SOURCE/SUB/NAME" = "*". In a manifest, a plain NAME is in the same
# (sub-)collection and "/PATH" is from the top of the same source.
# `import "@SOURCE/PATH/MODULE"` finds the plugin by its whole name.
C=$HOME/.config/luish
N=$HOME/src/nest
mkdir -p "$C" "$N/complete/all" "$N/complete/bio" "$N/lib/util" "$N/x" "$N/docs" "$N/.hidden"
printf '[dependencies]\nbio = "*"\ngui = "*"\n' > "$N/complete/all/plugin.toml"
printf '[dependencies]\n"/lib/util" = "*"\n' > "$N/complete/bio/plugin.toml"
cat > "$N/complete/bio/extension.rhai" <<'X'
import "@nest/lib/util/greet" as g;
print(g::hello("bio"));
X
echo 'echo gui' > "$N/complete/gui.lsh"
echo 'echo util' > "$N/lib/util/init.lsh"
echo 'fn hello(who) { `hello, ${who}` }' > "$N/lib/util/greet.rhai"
# x.lsh is a plugin, so x/ is no sub-collection; docs/ and .hidden/ have
# no plugins.
echo 'echo x' > "$N/x.lsh"
echo 'echo y' > "$N/x/y.lsh"
echo 'notes' > "$N/docs/README"
echo 'echo hidden' > "$N/.hidden/h.lsh"
cat > "$C/config.toml" <<'X'
[plugins.available]
nest = { path = "~/src/nest" }

[plugins.enabled]
nest.complete.all = "*"
X
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
p() { __luish_internal plugin "$@"; }
echo '--- startup: SOURCE.SUB.NAME, its siblings, and /PATH'
run '__luish_internal plugin list-loaded'
echo '--- the same, quoted'
printf '[plugins.available]\nnest = { path = "~/src/nest" }\n[plugins.enabled]\n"nest/complete/all" = "*"\n' \
    > "$C/config.toml"
run '__luish_internal plugin list-loaded'
rm "$C/config.toml"
printf '[plugins.available]\nnest = { path = "~/src/nest" }\n' > "$C/config.toml"
echo '--- list-available looks into sub-collections'
p list-available
echo '--- plugin load and unload SOURCE/PATH'
p load nest/complete/gui
p list-loaded
p unload nest/complete/gui
p list-loaded | wc -l
echo '--- @SOURCE/PATH/MODULE needs the whole name'
echo 'import "@other/lib/util/greet" as g;' > "$HOME/wrong.rhai"
$SH -c '__luish_internal plugin load nest/lib/util >/dev/null; __luish_internal plugin load ./wrong.rhai' 2>&1 |
    sed "s|$SH|luish|"
echo '--- errors'
for a in nest/x/y nest/nosuch/y nest/complete nest/a//b; do
    $SH -c "__luish_internal plugin load $a; echo \"status \$?\"" 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"
done
echo '--- plugin add SOURCE/PATH'
p add -y nest/complete/gui 2>&1 | sed "s|$HOME|~|g"
cat "$C/config.toml"
