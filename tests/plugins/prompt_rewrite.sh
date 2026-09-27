# prompt-rewrite hooks give the prompt instead of PS1, without parameter expansion
# but with `%` sequences under promptpercent. The most recently registered
# hook that returns a string wins; `()` or an error leaves it to the ones
# before it, then PS1. An interactive shell reading a pipe writes its
# prompts to stderr.
cat > status.rhai <<'P'
sh::hook("prompt-rewrite", || `[${sh::last_status()} $x %~] `);
P
cat > maybe.rhai <<'P'
// Only when $MAYBE is set; `$?` is kept even though the hook runs a command.
sh::hook("prompt-rewrite", || {
    sh::run("false");
    if sh::getvar("MAYBE") == () { () } else { "maybe> " }
});
P
cat > broken.rhai <<'P'
sh::hook("prompt-rewrite", || { throw "no prompt"; });
sh::hook("prompt-rewrite", || 42);
P
cat > quit.rhai <<'P'
sh::hook("prompt-rewrite", || { sh::run("exit 7"); "not shown" });
P
PS1='ps1> ' $SH -i +m <<'EOF2' > out 2>&1
plugin load ./status.rhai
x=1; false
setopt promptpercent
plugin load ./maybe.rhai
MAYBE=1
unset MAYBE
plugin load ./broken.rhai
plugin unload broken
echo 'two
lines'
plugin unload status maybe
plugin load ./quit.rhai
echo not reached
EOF2
echo "status $?"
sed "s|$SH|luish|; s|$HOME|H|g" out | cat -v
