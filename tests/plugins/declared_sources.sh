# A plugin's manifest can declare sources, in [available], as config.toml's
# plugins.available: its own dependencies can use them at once, and once
# `plugin sync` has recorded them in plugins.lock, so can plugins.enabled,
# `plugin load`, `plugin list-available` and `plugin add`. Only the enabled
# plugins' declarations count. config.toml's own sources win (with a warning
# from `plugin sync` and `plugin check` if they differ), two plugins that
# declare a source differently are an error, and std can't be declared.
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=a GIT_AUTHOR_EMAIL=a@b GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@b
export GIT_AUTHOR_DATE='2026-01-01T00:00:00Z' GIT_COMMITTER_DATE='2026-01-01T00:00:00Z'
C=$HOME/.config/luish
mkdir -p "$C" src/personal src/work src/extra/complete src/checkout/complete src/coll/p src/hidden grepo
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
cmd() { $SH -c "$1" 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"; }
for p in bio gui; do
    echo "echo $p" > src/extra/complete/$p.lsh
    echo "echo $p from the checkout" > src/checkout/complete/$p.lsh
done
echo 'echo personal' > src/personal/init.lsh
cat > src/personal/plugin.toml <<'X'
[available]
extra = { path = "../extra" }

[dependencies]
extra.complete.bio = "*"
X
cat > "$C/config.toml" <<'X'
[plugins.enabled]
personal = { path = "~/src/personal" }
X
echo '--- before plugin sync: the manifest can use its own source'
run '__luish_internal plugin list-loaded'
cmd '__luish_internal plugin load extra/complete/gui; echo "status $?"'
echo '--- plugin sync records it'
cmd '__luish_internal plugin sync; echo "status $?"'
sed "s|$HOME|~|g" "$C/plugins.lock"
echo '--- then plugin load, list-available and plugin add know it'
cmd '__luish_internal plugin list-available'
cmd '__luish_internal plugin load extra/complete/gui'
cmd '__luish_internal plugin add -y extra/complete/gui; echo "status $?"'
cat "$C/config.toml"
run '__luish_internal plugin list-loaded'
echo '--- plugins.enabled can name it before the plugin that declares it'
cat > "$C/config.toml" <<'X'
[plugins.enabled]
extra.complete.gui = "*"
personal = { path = "~/src/personal" }
X
rm "$C/plugins.lock"
cmd '__luish_internal plugin sync -q; echo "status $?"'
run '__luish_internal plugin list-loaded'
echo '--- but not without it'
printf '[plugins.enabled]\nextra.complete.gui = "*"\n' > "$C/config.toml"
cmd '__luish_internal plugin sync -q; echo "status $?"'
echo '--- config.toml wins, with a warning if its source is another'
cat > "$C/config.toml" <<'X'
[plugins.available]
extra = { path = "~/src/checkout" }

[plugins.enabled]
personal = { path = "~/src/personal" }
X
cmd '__luish_internal plugin sync; echo "status $?"'
cmd '__luish_internal plugin check; echo "status $?"'
grep -c available "$C/plugins.lock"
run '__luish_internal plugin list-loaded'
sed -i 's|checkout|extra|' "$C/config.toml"
cmd '__luish_internal plugin sync -q; echo "status $?"'
echo '--- two plugins that disagree'
echo 'echo work' > src/work/init.lsh
printf '[available]\nextra = { path = "../checkout" }\n' > src/work/plugin.toml
cat > "$C/config.toml" <<'X'
[plugins.enabled]
personal = { path = "~/src/personal" }
work = { path = "~/src/work" }
X
cmd '__luish_internal plugin sync -q; echo "status $?"'
echo '--- and that agree'
printf '[available]\nextra = { path = "~/src/extra" }\n' > src/work/plugin.toml
cmd '__luish_internal plugin sync -q; echo "status $?"'
grep -A1 available "$C/plugins.lock"
echo '--- a plugin that is only available declares nothing'
echo 'echo hidden' > src/hidden/h.lsh
echo 'echo p' > src/coll/p/init.lsh
printf '[available]\nhidden = { path = "../../hidden" }\n' > src/coll/p/plugin.toml
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }
X
cmd '__luish_internal plugin sync -q; echo "status $?"'
cmd '__luish_internal plugin list-available'
cmd '__luish_internal plugin load hidden/h; echo "status $?"'
echo '--- a git source: pinned, and plugin update knows its name'
echo 'echo g' > grepo/g.lsh
(cd grepo && git init -q -b main && git add -A && git commit -qm one)
echo 'echo personal' > src/personal/init.lsh
cat > src/personal/plugin.toml <<X
[available]
g = { git = "file://$HOME/grepo", branch = "main" }
X
printf '[plugins.enabled]\npersonal = { path = "~/src/personal" }\n' > "$C/config.toml"
cmd '__luish_internal plugin sync; echo "status $?"'
sed -n '/available/,$p' "$C/plugins.lock" | sed "s|$HOME|~|g"
cmd '__luish_internal plugin load g/g'
cmd '__luish_internal plugin update g; echo "status $?"'
echo '--- errors'
printf '[available]\nstd = { path = "../extra" }\n"a/b" = { path = "../extra" }\nx = 1\n' > src/personal/plugin.toml
cmd '__luish_internal plugin sync -q; echo "status $?"'
printf 'available = 1\n' > src/personal/plugin.toml
cmd '__luish_internal plugin sync -q; echo "status $?"'
