# `plugin run` runs Rhai code once, from a file or with -c, outside any
# plugin: nothing is loaded, a plugin of the same name stays loaded, and
# registering a hook, completer or command is an error. `argv` has the
# file (or -c) and the arguments, and the value of the last statement, if
# an integer or a boolean, is the status. Imports are relative to the file,
# or for -c to the current directory, and are read again for each run.
mkdir sub
echo 'fn twice(x) { x * 2 }' > sub/util.rhai
cat > sub/s.rhai <<'P'
import "util" as u;
print(`${argv}: ${u::twice(21)}`);
argv.len() - 1
P
cat > s.rhai <<'P'
sh::builtin("from_plugin", |argv| print("still loaded"));
P
__luish_internal plugin load ./s.rhai
__luish_internal plugin run sub/s.rhai 'a b' c; echo "file: $?"
__luish_internal plugin list-loaded
from_plugin
__luish_internal plugin run -c 'print(argv); argv[1] == "yes"' yes; echo "-c: $?"
__luish_internal plugin run -c 'false'; echo "false: $?"
__luish_internal plugin run -c '300'; echo "300: $?"
__luish_internal plugin run -c 'return 4; 5'; echo "return: $?"
__luish_internal plugin run -c '"a string"'; echo "string: $?"
__luish_internal plugin run -c 'let x = 1;'; echo "unit: $?"
__luish_internal plugin run -c 'import "sub/util" as u; u::twice(3)'; echo "import: $?"
echo 'fn twice(x) { x * 10 }' > sub/util.rhai
__luish_internal plugin run sub/s.rhai; echo "again: $?"
# It reaches the shell as an extension does.
__luish_internal plugin run -c 'sh::setvar("V", `${argv[1]}!`)' value; echo "V=$V"
__luish_internal plugin run -c 'sh::run("exit 6"); print("not reached")'; echo "not reached"
