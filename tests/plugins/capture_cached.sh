# sh::capture_cached returns what sh::capture does, from an earlier run of
# the same program and arguments, in the same directory and with the same
# PATH, if it was less than its first argument (in seconds) ago. Each run
# here adds a line to $HOME/runs, and prints how many there are.
mkdir sub
cat > cc.rhai <<'P'
let count = ["sh", "-c", "echo >> \"$HOME/runs\"; wc -l < \"$HOME/runs\"; echo err >&2"];
fn show(r) { print(`${r.status} [${r.out}] ${if "err" in r { r.err } else { "-" }}`); }
show(sh::capture_cached(60, count));
show(sh::capture_cached(60, count));
// Another standard error, directory or PATH runs it again.
show(sh::capture_cached(60, count, "return"));
show(sh::capture_cached(60, count, "return"));
show(sh::capture_cached(60, count, "merge"));
sh::run("cd sub");
show(sh::capture_cached(60, count));
sh::run("cd ..");
show(sh::capture_cached(60, count));
sh::setvar("PATH", sh::getvar("PATH") + ":/nonexistent");
show(sh::capture_cached(60, count));
// 0 never keeps it.
show(sh::capture_cached(0, count));
show(sh::capture_cached(0, count));
// A failure is kept too, for a while.
let fail = ["sh", "-c", "echo >> \"$HOME/runs\"; wc -l < \"$HOME/runs\"; exit 3"];
show(sh::capture_cached(60, fail));
show(sh::capture_cached(60, fail));
let r = sh::capture_cached(60, ["no-such-program"], "return");
print(`missing: ${r.status} ${r.err.contains("no-such-program: not found")}`);
for bad in [|| sh::capture_cached(-1, ["true"]), || sh::capture_cached(1, []), || sh::capture_cached(1, [1]),
            || sh::capture_cached(1, ["true"], "bogus")] {
    try { bad.call(); } catch (e) { print(e); }
}
P
__luish_internal plugin load ./cc.rhai 2>stderr
echo "load: $?"
cat stderr
