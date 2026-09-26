# --no-plugins makes `plugin load` do nothing.
echo 'print("loaded");' > p.rhai
$SH --no-plugins -c '__luish_internal plugin load ./p.rhai; echo "status $?"; __luish_internal plugin list-loaded'
$SH -c '__luish_internal plugin load ./p.rhai; echo "status $?"; __luish_internal plugin list-loaded'
