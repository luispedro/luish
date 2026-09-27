# `__luish_internal complete LINE` prints what Tab offers for the last word
# of LINE: each match as the text that replaces the word (with the space or
# `/` that ends a single match), then a tab and its description, if any.
# Status 1 without matches, or if the completer fails.
mkdir dir
touch 'a file' abc dir/inner
cat > frob.rhai <<'R'
sh::completer("frob", |words, i| {
    let cur = words[i];
    if cur == "boom" {
        throw "frob failed";
    }
    if cur.starts_with("--mode=") {
        return #{prefix: "--mode=", candidates: ["fast", "slow"]};
    }
    if i == 1 {
        return [
            #{value: "serve", desc: "Start the server"},
            "stop",
            #{value: "--mode=", suffix: ""},
        ];
    }
    ()
});
R
__luish_internal plugin load ./frob.rhai
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
c 'frob '
c 'frob s'
c 'frob --mode=f'
c 'frob serve a'
c "frob serve 'a"
c 'frob serve di'
c 'frob serve zzz'
c 'frob boom'
c 'ech'
c 'cd d'
c 'echo $HO'
c 'echo ~/a\ '
__luish_internal complete
echo "status $?"
__luish_internal complete a b
echo "status $?"
