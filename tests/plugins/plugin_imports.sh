# `import "@SOURCE/PLUGIN/MODULE"` imports MODULE.rhai from the directory of
# another plugin, which must be loaded (a dependency in plugin.toml, for
# instance): here std's completion engine, whose own imports (lib.rhai's
# `import "kinds"`) are relative to its files. The plugin is found by its
# name (std/completion), or without the source when it was loaded by path.
C=$HOME/.config/luish
mkdir -p "$C" ext/sub src/deep
printf '[plugins.available]\nstd = { path = "%s" }\n' "$STD_PLUGINS" > "$C/config.toml"
printf '[dependencies]\nstd.completion = "*"\n' > ext/plugin.toml
cat > ext/extension.rhai <<'X'
import "@std/completion/lib" as lib;
print(`at load time: ${lib::parse("-a, --all  all").opts[0].desc}`);
sh::completer("mytool", |words, i| {
    import "sub/spec" as spec;
    import "@std/completion/lib" as lib;
    lib::complete(spec::spec(), words, i)
});
X
cat > ext/sub/spec.rhai <<'X'
fn spec() {
    #{
        opts: "-a, --all  show all\n--color=WHEN  when to colour\n-o FILE  output",
        values: #{"--color": ["always", "never", "auto"]},
        args: ["dirs"],
    }
}
X
c() {
    echo "--- $1"
    __luish_internal complete "$1"
}
__luish_internal plugin load ./ext
echo "load $?"
__luish_internal plugin list-loaded
c 'mytool --c'
c 'mytool --color='
c 'mytool -'
c 'mytool s'
echo "--- std/completion loaded by name or by path"
$SH -c '__luish_internal plugin load std/completion; __luish_internal plugin load ./ext' 2>&1 | head -1
$SH -c '__luish_internal plugin load "$STD_PLUGINS/completion"; __luish_internal plugin load ./ext' 2>&1 | head -1
echo "--- errors"
echo 'import "@std/nosuch/lib" as l;' > notloaded.rhai
__luish_internal plugin load ./notloaded.rhai
echo "status $?"
echo 'import "@std/completion" as l;' > short.rhai
__luish_internal plugin load ./short.rhai
echo "status $?"
echo 'import "@std/completion/nosuch" as l;' > nomodule.rhai
__luish_internal plugin load ./nomodule.rhai
echo "status $?"
