# `__luish_internal plugin` is `plugin` in any shell.
__luish_internal plugin list-loaded
echo "list: $?"
__luish_internal plugin unload nosuch 2>/dev/null
echo "unload: $?"
__luish_internal plugin 2>/dev/null
echo "usage: $?"
