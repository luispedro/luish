# sh::matches matches as `case` does: *, ?, bracket expressions with
# classes and ranges, and a backslash that escapes; the whole string must
# match, and a leading `.` or a `/` is not special.
cat > m.rhai <<'P'
for t in [
    ["*.conf", "one.conf"], ["*.conf", "one.other"], ["*.conf", ".conf"], ["*", "a/b"],
    ["a?c", "abc"], ["a?c", "ac"], ["a?c", "abbc"], ["", ""], ["", "x"],
    ["[ab]*", "beta"], ["[!ab]*", "beta"], ["[[:digit:]]*", "1x"], ["[a-c]", "d"],
    ["*[!A-Za-z0-9._-]*", "my-prog_1.0"], ["*[!A-Za-z0-9._-]*", "a b"], ["*[!A-Za-z0-9._-]*", "é"],
    ["\\*", "*"], ["\\*", "x"], ["[", "["], ["a[", "a["], ["?", "é"], ["??", "é"],
] {
    print(`${t[0]} ${t[1]}: ${sh::matches(t[0], t[1])}`);
}
P
__luish_internal plugin load ./m.rhai
echo "load: $?"
# The same under case.
for t in '*.conf one.conf' '[!ab]* beta' '\* *' '?? é'; do
    set -f; set -- $t; set +f
    case $2 in $1) echo "case $t: true" ;; *) echo "case $t: false" ;; esac
done
