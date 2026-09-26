# A prompt hook that takes an argument is given the previous prompt: the
# one the hooks registered before it give, or else PS1 (parameter-expanded,
# but not yet %-expanded). What it returns replaces that prompt, and `()`
# (or an error) keeps it. A hook without a parameter doesn't run the hooks
# before it.
cat > base.rhai <<'P'
sh::hook("prompt", || { sh::write(2, "(base ran)"); "base %~ " });
P
cat > wrap.rhai <<'P'
sh::hook("prompt", |prev| `[${sh::last_status()}]` + prev);
P
cat > named.rhai <<'P'
fn prompt(prev) { if sh::getvar("KEEP") == () { "<" + prev + ">" } }
sh::hook("prompt", prompt);
P
cat > closure.rhai <<'P'
let n = 0;
sh::hook("prompt", |prev| { n += 1; `${n}:${prev}` });
P
cat > plain.rhai <<'P'
sh::hook("prompt", || "plain> ");
P
cat > broken.rhai <<'P'
sh::hook("prompt", |prev| { throw "no prompt"; });
P
PS1='ps1 $x> ' $SH -i +m <<'EOF2' > out 2>&1
x=1
plugin load ./wrap.rhai
false
plugin load ./base.rhai
setopt promptpercent
plugin unload base
plugin load ./named.rhai
KEEP=1
unset KEEP
plugin load ./closure.rhai
plugin load ./broken.rhai
plugin unload broken closure named
plugin load ./plain.rhai
plugin unload plain
plugin load ./base.rhai ./closure.rhai
EOF2
echo "status $?"
sed "s|$SH|luish|; s|$HOME|H|g" out | cat -v
