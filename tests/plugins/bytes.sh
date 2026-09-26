# Bytes that aren't UTF-8 round-trip through plugins.
d=$(printf 'dir\377\200x')
mkdir "$d"
cat > b.rhai <<'P'
sh::hook("chpwd", |from, to| {
    sh::setvar("SEEN", to);
    sh::setvar("LEN", `${to.len()}`);
});
P
__luish_internal plugin load ./b.rhai
cd "$d"
[ "$SEEN" = "$PWD" ] && echo same
[ "${SEEN#"$HOME/"}" = "$d" ] && echo "same name"
echo "chars in last component: $((LEN - ${#HOME} - 1))"
