# `import "NAME"` resolves NAME.rhai relative to the file that imports it:
# the extension's directory for extension.rhai, and the module's own
# directory for a module in a subdirectory, also in its functions and
# closures, wherever they are called from (and in a completer). Modules are
# cached by absolute path, so two files called b.rhai are two modules, and
# sub/b and sub/deeper/../b one.
mkdir -p p/sub p/sub/deeper
cat > p/extension.rhai <<'X'
import "sub/a" as a;
import "b" as b;
print(`top: ${b::who()}, ${a::who()}, ${a::via_b()}`);
print(`in a function of a: ${a::later()}`);
print(`deeper: ${a::deeper()}`);
sh::completer("top", |words, i| {
    import "b" as b;
    [b::who()]
});
sh::completer("fromsub", |words, i| a::complete());
print(`a closure of a: ${a::closure_c()}`);
X
echo 'fn who() { "p/b" }' > p/b.rhai
cat > p/sub/a.rhai <<'X'
import "b" as b;
fn who() { "p/sub/a" }
fn via_b() { b::who() }
fn later() {
    import "c" as c;
    c::who()
}
fn deeper() {
    import "deeper/d" as d;
    d::who()
}
fn complete() {
    import "b" as b;
    [b::who()]
}
fn closure_c() {
    let f = || {
        import "c" as c;
        c::who()
    };
    f.call()
}
X
printf '%s\n' 'print("p/sub/b runs once");' 'fn who() { "p/sub/b" }' > p/sub/b.rhai
echo 'fn who() { "p/sub/c" }' > p/sub/c.rhai
cat > p/sub/deeper/d.rhai <<'X'
import "../b" as b;
fn who() { "p/sub/deeper/d, then " + b::who() }
X
__luish_internal plugin load ./p
echo "load $?"
__luish_internal complete 'top '
__luish_internal complete 'fromsub '
echo "--- an absolute path"
echo "fn who() { \"abs\" }" > abs.rhai
echo "import \"$PWD/abs\" as m; print(m::who());" > q.rhai
__luish_internal plugin load ./q.rhai
echo "--- a missing module in a subdirectory"
mkdir -p r/sub
echo 'import "sub/m" as m;' > r/extension.rhai
echo 'import "nosuch" as n;' > r/sub/m.rhai
__luish_internal plugin load ./r
echo "status $?"
