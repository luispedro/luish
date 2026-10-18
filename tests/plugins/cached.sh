# sh::cached(key, ttl, files, f) returns what f() returned for the same key
# in the same plugin less than ttl seconds ago, if the files are unchanged,
# or else calls f again. Each call of f here counts in $runs.
printf 'one\n' > data
mkdir lib
cat > lib/parse.rhai <<'P'
// A module's function, as std's lib.rhai would cache what it parses.
fn table(text) {
    sh::cached("table:" + text, 300, || {
        sh::run("runs=$((runs + 1))");
        text.split(",")
    })
}
P
cat > a.rhai <<'P'
import "lib/parse" as parse;
fn count(tag) {
    sh::run("runs=$((runs + 1))");
    `${tag} ${sh::getvar("runs")}`
}
sh::builtin("a", |args| {
    let what = args[1];
    let r = switch what {
        "plain" => sh::cached("k", 60, || count("plain")),
        "other" => sh::cached("k2", 60, || count("other")),
        "file" => sh::cached("f", 60, ["data"], || count("file")),
        "files" => sh::cached("f", 60, ["data", "missing"], || count("files")),
        "zero" => sh::cached("z", 0, || count("zero")),
        "named" => sh::cached("n", 60, Fn("count").curry("named")),
        "table" => parse::table("x,y").len(),
        "mutate" => {
            let v = sh::cached("m", 60, || [1, 2]);
            v.push(3);
            v.len()
        }
        "fail" => {
            let r = ();
            try {
                sh::cached("e", 60, || { sh::run("runs=$((runs + 1))"); throw "boom" });
            } catch (e) {
                r = `caught ${e} ${sh::getvar("runs")}`;
            }
            r
        }
    };
    print(r);
    0
});
P
cat > b.rhai <<'P'
sh::builtin("b", |args| {
    print(sh::cached("k", 60, || { sh::run("runs=$((runs + 1))"); `b ${sh::getvar("runs")}` }));
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
echo '-- 0 is never kept'
a zero; a zero
echo '-- named functions and modules'
a named; a named
a table; a table
echo '-- a copy'
a mutate; a mutate
echo '-- an error is not kept'
a fail; a fail
echo '-- loading a plugin again drops its values'
__luish_internal plugin load ./a.rhai
a plain; b
echo '-- errors'
__luish_internal plugin run -c '
for bad in [|| sh::cached("k", -1, || 1), || sh::cached("k", 1, [1], || 1)] {
    try { bad.call(); } catch (e) { print(e); }
}'
echo "runs: $runs"
