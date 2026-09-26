# Errors in plugins are reported with the plugin's file; a failing hook
# doesn't stop the other hooks or cd.
mkdir d
__luish_internal plugin load ./missing.rhai
echo "missing: $?"
echo 'let x = ;' > syntax.rhai
__luish_internal plugin load ./syntax.rhai
echo "syntax: $?"
cat > top.rhai <<'P'
sh::hook("chpwd", |a, b| print("registered before the error"));
throw "top-level error";
P
__luish_internal plugin load ./top.rhai
echo "top: $?"
echo 'sh::hook("nosuchhook", || 1);' > kind.rhai
__luish_internal plugin load ./kind.rhai
echo "kind: $?"
cat > hooks.rhai <<'P'
fn failing(a, b) { let y = 1; y.nosuchmethod(); }
sh::hook("chpwd", |a, b| { throw "oops"; });
sh::hook("chpwd", Fn("failing"));
sh::hook("chpwd", |a, b| { sh::setvar("A", "x\x00y"); });
sh::hook("chpwd", |a, b| { sh::setvar("bad-name", "v"); });
sh::hook("chpwd", |a, b| { sh::setvar("RO", "v"); });
sh::hook("chpwd", |a, b| print("last hook runs"));
P
readonly RO=1
__luish_internal plugin load ./hooks.rhai
echo "hooks: $?"
__luish_internal plugin list-loaded
cd d
echo "cd: $? $RO"
__luish_internal plugin
echo "usage: $?"
__luish_internal plugin frobnicate
echo "usage: $?"
__luish_internal plugin unload nosuch
echo "unload: $?"
