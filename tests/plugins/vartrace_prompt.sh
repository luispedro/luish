# What prompt-vars hooks set is put back after the prompt, so variable
# tracing doesn't record it; what a prompt-rewrite hook sets stays, and is
# recorded.
cat > vars.rhai <<'P'
sh::hook("prompt-vars", || #{ x: "inner" });
sh::hook("prompt-rewrite", |prev| { sh::setvar("seen", "yes"); prev });
P
x=outer $SH -i +m -o vars.trace <<'EOF2' 2>/dev/null
plugin load ./vars.rhai
echo "$x"
where x seen
EOF2
