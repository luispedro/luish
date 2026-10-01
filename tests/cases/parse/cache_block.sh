# `__luish_cache env=(...) files=(...) { ... }`: outside the startup files
# of an interactive shell, the body runs as any `{ ... }` would. Its options
# aren't expanded then.
__luish_cache env=(PATH HOME) files=(~/a "$UNSET/b") {
    echo body
    x=1
}
echo "x=$x"
__luish_cache { false; }
echo "status $?"
__luish_cache
{
    echo newline before the brace
}
f() { __luish_cache files=(/etc/*) { echo "in f: $1"; }; }
f a
type __luish_cache
echo __luish_cache is a word here
for s in '__luish_cache env=(1) { :; }' '__luish_cache files=($(echo x)) { :; }' \
    '__luish_cache env=(A) env=(B) { :; }' '__luish_cache x=(A) { :; }' '__luish_cache ( : )'; do
    out=$($SH -c "$s" 2>&1)
    echo "status $?: Syntax${out#*Syntax}"
done
