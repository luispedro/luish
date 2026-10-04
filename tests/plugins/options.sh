# Plugin options: a plugin declares them in the [plugin-options] table of its
# plugin.toml (a type, str, int or boolean, and a default or required = true),
# and is given them in plugins.enabled or a dependency (NAME = { version =
# "*", options = { ... } }), or as OPTION=VALUE to plugin load. Its shell
# files see them in the associative array LUISH_PLUGIN_OPTIONS, and its
# extension through sh::plugin_options(). A plugin loaded twice must be given
# the same options (after the defaults), also when it is loaded already.
C=$HOME/.config/luish
P=$C/plugins
mkdir -p "$P/greet" "$HOME/src/coll/b" "$HOME/src/coll/a" "$HOME/src/coll/c" "$HOME/src/coll/needy"
run() { $SH -i -c "$1" 2>&1 | grep -v 'job control' | sed "s|$SH|luish|; s|$HOME|~|g"; }
cat > "$P/greet/plugin.toml" <<'X'
[plugin-options]
who = { type = "str", default = "world" }
times = { type = "int", default = 1 }
loud = { type = "boolean", required = true }
extra = { type = "str" }
X
cat > "$P/greet/init.lsh" <<'X'
for k in "${!LUISH_PLUGIN_OPTIONS[@]}"; do echo "init: $k=${LUISH_PLUGIN_OPTIONS[$k]}"; done
if ${LUISH_PLUGIN_OPTIONS[loud]}; then echo "HELLO, ${LUISH_PLUGIN_OPTIONS[who]}"; fi
X
cat > "$P/greet/extension.rhai" <<'X'
let o = sh::plugin_options();
print(`rhai: ${o.who} ${type_of(o.times)} ${o.times + 1} ${type_of(o.loud)} ${o.loud} ${"extra" in o}`);
sh::builtin("greet", |argv| print(`hi, ${sh::plugin_options().who}`));
X
echo 'echo "post-rc: ${LUISH_PLUGIN_OPTIONS[who]}"' > "$P/greet/post-rc.lsh"
# b takes an option; a and c depend on it.
printf '[plugin-options]\nlevel = { type = "int", default = 0 }\n' > "$HOME/src/coll/b/plugin.toml"
echo 'echo "b: level ${LUISH_PLUGIN_OPTIONS[level]}"' > "$HOME/src/coll/b/init.lsh"
printf '[dependencies]\nb = { version = "*", options = { level = 2 } }\n' > "$HOME/src/coll/a/plugin.toml"
echo 'echo a' > "$HOME/src/coll/a/init.lsh"
printf '[dependencies]\nb = { options = { level = 2 } }\n' > "$HOME/src/coll/c/plugin.toml"
echo 'echo c' > "$HOME/src/coll/c/init.lsh"
printf '[plugin-options]\nkey = { type = "str", required = true }\n' > "$HOME/src/coll/needy/plugin.toml"
echo 'echo "needy: ${LUISH_PLUGIN_OPTIONS[key]}"' > "$HOME/src/coll/needy/init.lsh"
echo 'echo "plain: ${#LUISH_PLUGIN_OPTIONS[@]} options"' > "$HOME/src/coll/plain.lsh"
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }

[plugins.enabled]
greet = { version = "*",
          options = { who = "luish", loud = true } }
coll.a = "*"
coll.c = "*"         # gives b the same options as a: b loads once
X
echo '--- startup: options, defaults, and the same options for b twice'
run 'echo "after: ${LUISH_PLUGIN_OPTIONS-unset}"; greet'
echo '--- the startup cache restores the options'
run 'echo "after: ${LUISH_PLUGIN_OPTIONS-unset}"; greet'
echo '--- savestate records them'
$SH -c '__luish_internal plugin load greet loud=false times=3 >/dev/null; __luish_internal savestate' |
    grep '^__luish_internal plugin restore' | sed "s|$HOME|~|"
echo '--- plugin load: OPTION=VALUE after each plugin, as the types declared'
$SH -c '__luish_internal plugin load greet who=sh times=-2 loud=true coll/plain coll/b level=7'
echo '--- inconsistent options for a dependency'
printf '[dependencies]\nb = { options = { level = 3 } }\n' > "$HOME/src/coll/c/plugin.toml"
run ':'
echo '--- the defaults count: no options is level = 0'
printf '[dependencies]\nb = "*"\n' > "$HOME/src/coll/c/plugin.toml"
run ':'
echo '--- a plugin loaded already, with other options'
$SH -c '__luish_internal plugin load coll/b level=1 >/dev/null
__luish_internal plugin load coll/a; echo "status $?"
__luish_internal plugin load coll/b level=1 >/dev/null && echo same
__luish_internal plugin unload coll/b; __luish_internal plugin load coll/a >/dev/null && echo "after unload"' 2>&1 |
    sed "s|$SH|luish|"
echo '--- errors in the options given'
for args in 'greet' 'greet loud=yes' 'greet loud=true times=x' 'greet loud=true nosuch=1' \
    'greet loud=true loud=false' 'coll/plain x=1' 'coll/needy' 'coll/needy key=' 'x=1 greet'; do
    $SH -c "__luish_internal plugin load $args; echo \"status \$?\"" 2>&1 | sed "s|$SH|luish|"
done
echo '--- errors in plugin.toml and config.toml'
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }
[plugins.enabled]
greet = { options = { loud = "yes" } }
coll.b = { version = "1", options = { level = 1 } }
"coll/needy" = { version = "*", option = {} }
"coll/a" = { options = { who = 1.5 } }
"coll/c" = { options = [] }
X
run ':'
cat > "$HOME/src/coll/b/plugin.toml" <<'X'
[plugin-options]
a = { type = "float" }
b = { type = "int", default = "0" }
c = { type = "str", default = "", required = true }
d = { default = 1 }
"e f" = { type = "str" }
g = { type = "str", help = "?" }
h = "str"
ok = { type = "boolean" }
X
$SH -c '__luish_internal plugin load coll/b ok=true; echo "status $?"' 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"
echo '--- an inline source with options, and a nested entry'
printf '[plugin-options]\nlevel = { type = "int", default = 0 }\n' > "$HOME/src/coll/b/plugin.toml"
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }
[plugins.enabled]
bee = { path = "~/src/coll", plugin = "b", options = { level = 4 } }
coll.needy = { options = { key = "k" } }
X
run ':'
echo '--- plugin sync checks the options of the enabled plugins only'
cat > "$C/config.toml" <<'X'
[plugins.available]
coll = { path = "~/src/coll" }
[plugins.enabled]
coll.plain = "*"
X
$SH -c '__luish_internal plugin sync -q; echo "status $?"'
printf '[plugins.available]\ncoll = { path = "~/src/coll" }\n[plugins.enabled]\ncoll.needy = "*"\n' > "$C/config.toml"
$SH -c '__luish_internal plugin sync -q; echo "status $?"' 2>&1 | sed "s|$SH|luish|; s|$HOME|~|g"
