# sh::capture runs shell code in a subshell, like $(...), and returns its
# status and its output without trailing newlines.
cat > cap.rhai <<'P'
let r = sh::capture("printf 'a b\n\n\n'; exit 3");
print(`status ${r.status}, out [${r.out}]`);
let r = sh::capture("X=changed; echo $X; echo to-stderr >&2");
print(`out ${r.out}, X is ${sh::getvar("X")}`);
let r = sh::capture("true");
print(`empty [${r.out}] ${r.status}`);
let r = sh::capture("printf '\\377'");
print(`bytes ${r.out.len} ${sh::capture("printf %s " + r.out + " | od -An -tx1").out}`);
P
X=kept
__luish_internal plugin load ./cap.rhai 2>/dev/null
echo "load: $? X=$X"
