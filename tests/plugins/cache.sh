# sh::cache_get(key, files) returns what sh::cache_put(key, ttl, files, value)
# kept for the same key in the same plugin less than ttl seconds ago, if the
# files are unchanged (as they were at the miss before the put), or else ().
# Each computation here counts in $runs.
printf 'one\n' > data
mkdir lib
cat > lib/parse.rhai <<'P'
// A module's function, as std's lib.rhai keeps what it parses: for the
// plugin that calls it.
fn table(text) {
    let key = "table:" + text;
    sh::cache_get(key) ?? sh::cache_put(key, 300, fields(text))
}
fn fields(text) {
    sh::run("runs=$((runs + 1))");
    text.split(",")
}
P
cat > a.rhai <<'P'
import "lib/parse" as parse;
fn count(tag) {
    sh::run("runs=$((runs + 1))");
    `${tag} ${sh::getvar("runs")}`
}
fn kept(key, ttl, files, tag) {
    sh::cache_get(key, files) ?? sh::cache_put(key, ttl, files, count(tag))
}
sh::builtin("a", |args| {
    let what = args[1];
    let r = switch what {
        "plain" => sh::cache_get("k") ?? sh::cache_put("k", 60, count("plain")),
        "other" => sh::cache_get("k2") ?? sh::cache_put("k2", 60, count("other")),
        "file" => kept("f", 60, ["data"], "file"),
        "files" => kept("f", 60, ["data", "missing"], "files"),
        "zero" => kept("z", 0, [], "zero"),
        "unit" => sh::cache_get("u") ?? sh::cache_put("u", 60, ()) ?? "not kept",
        "table" => parse::table("x,y").len(),
        "mutate" => {
            let v = sh::cache_get("m") ?? sh::cache_put("m", 60, [1, 2]);
            v.push(3);
            v.len()
        }
        "race" => {
            // data changes while the value is first computed: it is kept,
            // but stale at the next get.
            let v = sh::cache_get("r", ["data"]);
            if v == () {
                sh::run("[ -e changed ] || { echo more >> data; touch changed; }");
                v = sh::cache_put("r", 60, ["data"], count("race"));
            }
            v
        }
    };
    print(r);
    0
});
P
cat > b.rhai <<'P'
sh::builtin("b", |args| {
    print(sh::cache_get("k") ?? sh::cache_put("k", 60, { sh::run("runs=$((runs + 1))"); `b ${sh::getvar("runs")}` }));
    0
});
P
runs=0
__luish_internal plugin load ./a.rhai ./b.rhai
a plain; a plain; a other; a plain
# Each plugin has its own keys.
b; b
echo '-- files'
a file; a file
printf 'two\n' > data
a file; a file
# Other files are another value.
a files; a files
touch missing
a files
rm data
a file; a file
echo '-- a file that changes while the value is computed'
touch data
a race; a race; a race
echo '-- 0 is never kept, nor ()'
a zero; a zero
a unit
echo '-- modules'
a table; a table
echo '-- a copy'
a mutate; a mutate
echo '-- loading a plugin again drops its values'
__luish_internal plugin load ./a.rhai
a plain; b
echo '-- errors'
__luish_internal plugin run -c '
for bad in [|| sh::cache_put("k", -1, 1), || sh::cache_put("k", 1, [1], 1), || sh::cache_get("k", [1])] {
    try { bad.call(); } catch (e) { print(e); }
}'
echo "runs: $runs"
