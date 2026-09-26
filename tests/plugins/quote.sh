# sh::quote quotes a string, or the strings of an array, for the shell.
cat > q.rhai <<'P'
print(sh::quote("it's a $test"));
print(sh::quote(["printf", "[%s]", "a b", "", "'"]));
print(sh::capture(sh::quote(["printf", "[%s]", "a b", "", "it's"])).out);
print(sh::quote([]) == "");
try { sh::quote([1]); } catch (e) { print(e); }
P
__luish_internal plugin load ./q.rhai 2>/dev/null
echo "load: $?"
