# Loading a plugin again replaces it; unloading removes its hooks.
mkdir d
echo 'sh::hook("chpwd", |a, b| print("v1"));' > p.rhai
echo 'sh::hook("chpwd", |a, b| print("other"));' > other.rhai
__luish_internal plugin load ./p.rhai ./other.rhai
cd d; cd ..
echo 'sh::hook("chpwd", |a, b| print("v2"));' > p.rhai
__luish_internal plugin load ./p.rhai
__luish_internal plugin list-loaded
cd d; cd ..
__luish_internal plugin unload p
echo "unload: $?"
__luish_internal plugin list-loaded
cd d; cd ..
__luish_internal plugin unload other
__luish_internal plugin list-loaded
cd d
echo end
