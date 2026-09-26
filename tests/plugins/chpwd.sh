# chpwd hooks run after each successful cd (also by pushd and popd), with
# the old and new directory.
mkdir -p "$HOME/.config/luish/plugins" d1 d2/sub
cat > "$HOME/.config/luish/plugins/dirs.rhai" <<'P'
let count = 0;
sh::hook("chpwd", |from, to| {
    count += 1;
    print(`chpwd ${count}: ${from} -> ${to} (PWD=${sh::getvar("PWD")})`);
    sh::setvar("LAST_TO", to);
});
fn named(from, to) {
    sh::write(1, "named hook, $? is " + sh::last_status() + "\n");
}
sh::hook("chpwd", Fn("named"));
P
main() {
__luish_internal plugin load dirs
echo "load: $?"
__luish_internal plugin list
top=$PWD
cd d1
echo "cd: $? $LAST_TO"
false
cd ../d2
echo "\$? kept: $?"
cd -
echo "failed cd:"
cd /nonexistent 2>/dev/null
echo "status $?"
CDPATH=$top/d2 cd sub
cd "$top"
echo "subshell:"
(cd d1)
echo "back in $PWD, LAST_TO=$LAST_TO"
echo "pushd and popd:"
pushd -q d1
pushd -q "$top/nonexistent" 2>/dev/null
popd -q
}
# The temporary directory changes from run to run.
main | sed "s|$HOME|HOME|g"
