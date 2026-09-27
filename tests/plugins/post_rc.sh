# Startup order in an interactive shell: config.toml's options, the plugins
# it enables, the files of rc.d, then each plugin's post-rc.lsh and the
# extensions' post-rc hooks (for plugins loaded so far, in the order they
# were loaded), then rc.d's _uncached.lsh. A plugin loaded later (luishrc,
# the prompt) runs its post-rc.lsh and hooks right after its rc.lsh. The
# cache keeps what post-rc.lsh did, and runs the hooks again, as it loads
# the extensions again. Scripts run neither.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$C/rc.d" "$P/p" "$P/q" "$P/r"
cat > "$C/config.toml" <<'X'
[plugins.enabled]
p = "*"
X
echo 'echo "p init.lsh (rc.d has run: ${from_rc-no})"' > "$P/p/init.lsh"
echo 'echo "p rc.lsh"' > "$P/p/rc.lsh"
echo 'echo "p post-rc.lsh (rc.d has run: ${from_rc-no}, in $LUISH_PLUGIN_NAME)"; p_post=done' > "$P/p/post-rc.lsh"
cat > "$P/p/extension.rhai" <<'X'
print("p extension.rhai");
sh::hook("post-rc", || print(`p post-rc hook (post-rc.lsh: ${sh::getvar("p_post")})`));
X
echo 'echo "q post-rc.lsh"' > "$P/q/post-rc.lsh"
echo 'sh::hook("post-rc", || print("q post-rc hook"));' > "$P/q/extension.rhai"
echo 'echo "r rc.lsh"' > "$P/r/rc.lsh"
echo 'echo "r post-rc.lsh"' > "$P/r/post-rc.lsh"
echo 'sh::hook("post-rc", || print("r post-rc hook"));' > "$P/r/extension.rhai"
cat > "$C/rc.d/a.lsh" <<'X'
echo "rc.d (loaded: $(__luish_internal plugin list-loaded))"
from_rc=yes
__luish_internal plugin load q
X
echo 'echo "_uncached.lsh"' > "$C/rc.d/_uncached.lsh"
echo '__luish_internal plugin load r' > "$C/luishrc"
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control'; }
echo '--- builds the cache'
run 'echo "prompt: $p_post"'
echo '--- uses it'
run 'echo "prompt: $p_post"'
echo '--- a script'
$SH -c '__luish_internal plugin load ./.config/luish/plugins/p; echo "script: ${p_post-unset}"'
