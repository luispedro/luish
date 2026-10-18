# A completer may hand the line over to the completer of another command, by
# returning #{delegate: WORDS, index: I} (WORDS[I] is the word being
# completed), with `dir`, the directory that completer runs in. The handed-over
# completer's answer is what Tab offers: candidates, a prefix, or () for
# filenames. Loops and bad maps are errors of the completer that hands over.
start=$(pwd)
mkdir -p d/inner
touch afile d/dfile d/inner/deep
cat > wrap.rhai <<'R'
// `wrap [-C DIR] CMD ARGS...`: CMD's completion, in DIR.
sh::completer("wrap", |words, i| {
    if words[1] == "-C" {
        if i < 4 {
            return [];
        }
        return #{delegate: words.extract(3), index: i - 3, dir: words[2]};
    }
    if i == 1 {
        return ["inner", "other"];
    }
    #{delegate: words.extract(1), index: i - 1}
});
sh::completer("inner", |words, i| {
    let cur = words[i];
    if cur.starts_with("--to=") {
        return #{prefix: "--to=", candidates: ["origin", "backup"]};
    }
    if cur == "here" {
        let d = sh::cwd().split("/");
        return [`here:${d[-1]}`];
    }
    if words[1] == "files" {
        return ();
    }
    [`inner:${words}:${i}`]
});
sh::completer("loop1", |words, i| #{delegate: ["loop2"] + words.extract(1), index: i});
sh::completer("loop2", |words, i| #{delegate: ["loop1"] + words.extract(1), index: i});
sh::completer("bad", |words, i| {
    switch words[1] {
        "index" => #{delegate: ["inner", "x", "y"], index: 1},
        "empty" => #{delegate: [], index: 0},
        "words" => #{delegate: ["inner", 1], index: 1},
        "dir" => #{delegate: ["inner", words[i]], index: 1, dir: 3},
        _ => (),
    }
});
R
__luish_internal plugin load ./wrap.rhai
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
c 'wrap '
c 'wrap inner a '
c 'wrap inner a b'
c 'wrap inner --to=o'
echo "=== in another directory, then back"
c 'wrap -C d inner here'
c 'wrap -C d inner files '
c 'wrap -C d inner files df'
c 'wrap -C . inner files w'
c 'wrap -C d inner files inner/'
c 'wrap -C d inner d'
c 'wrap -C d/inner/.. inner here'
c 'wrap -C /no/such/dir inner here'
[ "$(pwd)" = "$start" ] && echo "back in the starting directory"
c 'wrap -C d inner here'
echo "=== a command without a completer: filenames"
c 'wrap other a'
c 'wrap inner files a'
echo "=== errors"
c 'loop1 x'
c 'bad index y'
c 'bad empty y'
c 'bad words y'
c 'bad dir y'
