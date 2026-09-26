# savestate records loaded plugins, by absolute path, so that the startup
# cache loads them again.
mkdir d
echo 'sh::hook("chpwd", |a, b| print("hook"));' > p.rhai
__luish_internal plugin load ./p.rhai
__luish_internal savestate > state
grep "^__luish_internal plugin" state | sed "s|$HOME|HOME|"
cd d
$SH -c '. ../state; __luish_internal plugin list-loaded; cd /'
