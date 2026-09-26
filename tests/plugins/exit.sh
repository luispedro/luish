# Shell code run by a hook can exit the shell, which stops the plugin.
cat > exit.rhai <<'P'
sh::hook("chpwd", |a, b| {
    let s = sh::run("echo in hook; false");
    print(`run returned ${s}`);
    sh::run("exit 7");
    print("not reached");
});
sh::hook("chpwd", |a, b| print("not reached either"));
P
trap 'echo "EXIT trap, status $?"' EXIT
__luish_internal plugin load ./exit.rhai
cd /
echo "not reached after cd"
