# A `__luish_cache` block in a plugin that config.toml enables, or that a
# file of rc.d loads, is cached as an entry of its own, and what it is keyed
# on (its variables, its files, what it read with `.`) is added to the key of
# the entry it ran in: that entry is rebuilt when they change, and the
# block's own entry is used when the entry is rebuilt for another reason. A
# block that returns is saved neither on its own nor in the entry.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$P/p" "$P/q"
# q, loaded first (p depends on it), assigns a variable that may have been
# inherited with the same value: still saved, although p's block is built
# in between.
echo 'export SAME=1' > "$P/q/init.lsh"
printf '[dependencies]\nq = "*"\n' > "$P/p/plugin.toml"
cat > "$P/p/init.lsh" <<'X'
echo 'p: init.lsh runs'
__luish_cache env=(TOOL FAIL) files=("$HOME/dep") {
    echo "p: block built for $TOOL"
    . "$HOME/lib.lsh"
    P_VAR=$TOOL$(cat "$HOME/dep" 2>/dev/null)$LIB
    [ -z "$FAIL" ] || return 1
}
X
echo 'LIB=-lib1' > lib.lsh
printf '[plugins.enabled]\np = "*"\n' > "$C/config.toml"
show() {
    $SH -i -c 'echo "P_VAR=$P_VAR SAME=$SAME"' 2>&1 | grep -v 'job control'
}
echo '--- enabled in config.toml: built, then restored'
TOOL=a SAME=1 show
TOOL=a show
echo '--- its variable, its file and what it read with . rebuild the entry'
TOOL=b show
echo 1 > dep
TOOL=b show
echo 'LIB=-lib2' > lib.lsh
TOOL=b show
TOOL=b show
echo '--- config.toml changes: the entry is rebuilt, the block restored'
echo '# a comment' >> "$C/config.toml"
TOOL=b show
echo '--- a block that returns is not saved, nor is the entry'
TOOL=c FAIL=1 show
TOOL=c FAIL=1 show
echo '--- check-cache'
TOOL=b $SH -c '__luish_internal check-cache rc' | sed '/^rc:/d; /generated/d'
echo '--- loaded by a file of rc.d'
printf '' > "$C/config.toml"
mkdir -p "$C/rc.d"
echo 'plugin load p' > "$C/rc.d/p.lsh"
TOOL=d show
TOOL=d show
TOOL=e show
