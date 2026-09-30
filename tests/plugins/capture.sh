# sh::capture_sh runs shell code in a subshell, like $(...), and returns its
# status and its output without trailing newlines.
cat > cap.rhai <<'P'
let r = sh::capture_sh("printf 'a b\n\n\n'; exit 3");
print(`status ${r.status}, out [${r.out}]`);
let r = sh::capture_sh("X=changed; echo $X; echo to-stderr >&2");
print(`out ${r.out}, X is ${sh::getvar("X")}`);
let r = sh::capture_sh("true");
print(`empty [${r.out}] ${r.status}`);
let r = sh::capture_sh("printf '\\377'");
print(`bytes ${r.out.len} ${sh::capture_sh("printf %s " + r.out + " | od -An -tx1").out}`);
P
X=kept
__luish_internal plugin load ./cap.rhai 2>/dev/null
echo "load: $? X=$X"

# sh::capture runs a program with its arguments as they are (no word is shell
# code, and a function of the same name isn't called), stdin from /dev/null,
# and stderr discarded, or as its second argument says.
printf() { echo function; }
cat > argv.rhai <<'P'
let r = sh::capture(["printf", "[%s]\n", "a b", "it's", "$X;x", "*"]);
print(`${r.status} ${r.out} err: ${"err" in r}`);
let r = sh::capture(["sh", "-c", "echo out; echo err >&2; exit 3"]);
print(`discard: ${r.status} [${r.out}]`);
let r = sh::capture(["sh", "-c", "echo out; echo err >&2; exit 3"], "return");
print(`return: ${r.status} [${r.out}] [${r.err}]`);
let r = sh::capture(["sh", "-c", "echo out; echo err >&2"], "merge");
print(`merge: [${r.out}]`);
let r = sh::capture(["sh", "-c", "echo inherited >&2; echo out"], "inherit");
print(`inherit: [${r.out}]`);
let r = sh::capture(["cat"]);
print(`stdin: [${r.out}] ${r.status}`);
let r = sh::capture(["env", "X=set", "sh", "-c", "echo $X"]);
print(`env: ${r.out}`);
let r = sh::capture(["no-such-program"], "return");
print(`missing: ${r.status} ${r.err.contains("no-such-program: not found")}`);
let r = sh::capture(["sh", "-c", "head -c 100000 /dev/zero | tr '\\0' e >&2; head -c 100000 /dev/zero | tr '\\0' o"], "return");
print(`both pipes: ${r.out.len} ${r.err.len}`);
for bad in [|| sh::capture("echo hi"), || sh::capture([]), || sh::capture([1]), || sh::capture(["true"], "bogus")] {
    try { bad.call(); } catch (e) { print(e); }
}
P
echo in | __luish_internal plugin load ./argv.rhai 2>stderr
echo "load: $?"
cat stderr
