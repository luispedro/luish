# sh::getarray gives a variable's elements ("${a[@]}"; a string is one
# element) and sh::getmap an associative array; sh::setvar given an array or
# a map assigns an array or an associative array, keeping the variable's
# attributes and scope. sh::getvar still gives $a, the first element.
cat > get.rhai <<'P'
print(`getvar: ${sh::getvar("a")}`);
print(`a: ${sh::getarray("a")}`);
print(`s: ${sh::getarray("s")}`);
print(`e: ${sh::getarray("e")}`);
print(`h: ${sh::getarray("h")}`);
print(`path: ${sh::getarray("path")}`);
print(`pipestatus: ${sh::getarray("pipestatus")}`);
print(`unset: ${sh::getarray("nosuch") ?? "()"} ${sh::getmap("nosuch") ?? "()"}`);
print(`map: ${sh::getmap("h")}, of an array: ${sh::getmap("a") ?? "()"}`);
P
cat > set.rhai <<'P'
sh::setvar("b", ["x y", "", "z"]);
sh::setvar("m", #{k1: "v1", "a b": "c"});
sh::setvar("e", []);
sh::setvar("path", ["/p1", "/p2"]);
sh::setvar("n", ["1+2", "7"]);
sh::setvar("u", ["q", "p", "q"]);
// Errors leave the variables as they were.
let bad = [
    ["r", ["x"]], ["r", #{k: "v"}], ["bad-name", ["x"]], ["bad-name", #{k: "v"}],
    ["n", [1]], ["n", ["a\x00"]], ["n", #{k: 2}], ["n", #{"k\x00": "v"}], ["n", ["1+"]],
];
for b in bad {
    try { sh::setvar(b[0], b[1]); } catch (e) { print(`${b[0]}: ${e}`); }
}
P
a=(1 "2 3") s=str e=()
typeset -A h
h=([k]=v [x]=y)
PATH=/bin:/usr/bin
true | false
__luish_internal plugin load ./get.rhai
readonly r=1
typeset -i n
typeset -U u
__luish_internal plugin load ./set.rhai
typeset -p b m e n u
echo "PATH=$PATH"
# In a function, a local is set, not the global.
echo 'sh::setvar("q", ["local"]);' > local.rhai
f() {
    typeset -a q
    __luish_internal plugin load ./local.rhai
    echo "in f: ${q[*]}"
}
q=global
f
echo "q=$q"
