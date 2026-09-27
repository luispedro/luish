# `__luish_internal check-cache` with config.toml and the plugins it enables:
# the copy of the shell restores the plugins, and the shell loads them, so
# the two agree. The check prints nothing that the extensions print.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$C/rc.d" "$P/p"
cat > "$C/config.toml" <<'X'
[options]
glob.star = true
[plugins.enabled]
p = "*"
X
echo 'p_var=$(cat "$HOME/value"); alias p_alias="echo p"' > "$P/p/init.lsh"
echo 'print("p extension.rhai"); sh::hook("post-rc", || print("p post-rc hook"));' > "$P/p/extension.rhai"
echo 'from_rc=yes' > "$C/rc.d/a.lsh"
echo 1 > value
$SH -i -c 'echo "$p_var"' 2>&1 | grep -v 'job control'
$SH -c '__luish_internal check-cache -q; echo "status $?"'
echo 2 > value
$SH -c '__luish_internal check-cache -q' | sed '/^rc:/d; /generated/d'
$SH -i -c 'echo "$p_var"' 2>&1 | grep -v 'job control'
echo '--- config.toml changed'
sed 's/true/false/' "$C/config.toml" > tmp
cat tmp > "$C/config.toml"
$SH -c '__luish_internal check-cache -q' | sed '/^rc:/d; /generated/d; s|'"$HOME"'|~|'
$SH -c '__luish_internal check-cache -q; echo "status $?"'
