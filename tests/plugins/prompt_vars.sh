# prompt-vars hooks return a map of variables, and a directory plugin's
# prompt-vars.lsh sets variables in shell, which PS1 then uses. They run
# before each PS1 prompt, plugin by plugin in the order they were loaded,
# so later ones see and override earlier ones; afterwards the variables are
# as they were. prompt-rewrite hooks run after them and see them. `$?` is
# kept. An interactive shell reading a pipe writes its prompts to stderr.
cat > vars.rhai <<'P'
sh::hook("prompt-vars", || #{
    where: "rhai",
    x: "inner",
    n: 42,
    ok: sh::last_status() == 0,
    gone: (),
});
P
mkdir shell
cat > shell/prompt-vars.lsh <<'P'
# Sees $? of the last command, the earlier plugins' variables, and its own
# directory; none of what it sets outlives the prompt.
st=$?
where="$where+lsh(${LUISH_PLUGIN_NAME}:${LUISH_PLUGIN_DIR##*/})"
readonly ro=1
P
cat > broken.rhai <<'P'
sh::hook("prompt-vars", || 42);
sh::hook("prompt-vars", || #{"bad name": "a", arr: [1], ro2: "b", nul: "a\x00b", fine: "yes"});
sh::hook("prompt-vars", || { throw "no vars"; });
P
cat > rewrite.rhai <<'P'
sh::hook("prompt-rewrite", |prev| {
    sh::setvar("seen", sh::getvar("where") ?? "none");
    `{${prev}}`
});
P
mkdir quit
cat > quit/prompt-vars.lsh <<'P'
exit 3
P
PS1='[$where $x $n $ok ${gone-unset} ${st-} ${fine-}] ' x=outer gone=here $SH -i +m <<'EOF2' > out 2>&1
plugin load ./vars.rhai
false
echo "after: $? $where $x $n ${gone-unset}"
plugin load ./shell
echo "after: ${st-unset} ${ro-unset} ${LUISH_PLUGIN_DIR-unset}"; readonly ro2=1
plugin load ./broken.rhai
plugin unload broken
plugin load ./rewrite.rhai
echo "seen: $seen"
plugin unload vars rewrite
plugin load ./vars.rhai
plugin load ./quit
echo not reached
EOF2
echo "status $?"
sed "s|$SH|luish|; s|$HOME|H|g" out | cat -v
