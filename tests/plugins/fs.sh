# The fs module: file tests, times, contents and directory listings.
mkdir -p d/sub a/b/c
echo hello > f
: > empty
ln -s f link
ln -s nowhere broken
mkfifo fifo
touch -d '2020-01-01 00:00:00 UTC' old
touch -d '2021-01-01 00:00:00 UTC' new
mkdir l l/dir
ln -s d ld
ln -s ../../f d/sub/rel
touch l/b l/a l/.hidden
printf 'x\377y' > "l/$(printf 'bin\377')"
cat > fs.rhai <<'P'
for p in ["f", "d", "link", "broken", "fifo", "missing", "/dev/null", ""] {
    print(`${p}: exists=${fs::exists(p)} file=${fs::is_file(p)} dir=${fs::is_dir(p)} link=${fs::is_link(p)} kind=${fs::kind(p) ?? "none"}`);
}
print(`readable: ${fs::is_readable("f")} ${fs::is_readable("missing")}`);
print(`writable: ${fs::is_writable("f")} ${fs::is_writable("missing")}`);
print(`executable: ${fs::is_executable("d")} ${fs::is_executable("f")}`);
print(`size: ${fs::size("f")} ${fs::size("empty")} ${fs::size("missing") ?? "none"}`);
print(`mtime: ${fs::mtime("old")} ${fs::mtime("missing") ?? "none"}`);
print(`newer: ${fs::newer("new", "old")} ${fs::newer("old", "new")} ${fs::newer("new", "missing")} ${fs::newer("missing", "new")} ${fs::newer("new", "new")}`);
print(`older: ${fs::older("old", "new")} ${fs::older("new", "old")} ${fs::older("missing", "new")} ${fs::older("new", "missing")}`);
print(`read_file: ${fs::read_file("f")}${fs::read_file("missing") ?? "none"} ${fs::read_file("d") ?? "none"}`);
print(`list_dir: ${fs::list_dir("l")}`);
print(`list_dir: ${fs::list_dir("d")} ${fs::list_dir("f") ?? "none"} ${fs::list_dir("") ?? "none"}`);
print(`readlink: ${fs::readlink("link")} ${fs::readlink("broken")} ${fs::readlink("f") ?? "none"}`);
// realpath follows links in every component (a relative target is taken
// from the link's directory), and `..` after a link is the target's parent.
let home = fs::realpath(sh::getvar("HOME"));
for p in ["f", "link", "ld/sub/rel", "ld/sub/../sub/rel", "ld/sub/../f", ".", "broken", "missing", ""] {
    let r = fs::realpath(p);
    print(`realpath ${p}: ${if r == () { "none" } else { r.replace(home, "HOME"); r }}`);
}
// Bytes that aren't UTF-8 round-trip.
let name = "l/" + fs::list_dir("l").filter(|n| n.starts_with("bin"))[0];
print(`binary name: ${fs::is_file(name)} ${fs::read_file(name).len()}`);
sh::setvar("BIN", fs::read_file(name));
// find_up looks in the directory and its parents.
let home = sh::getvar("HOME");
print(`find_up: ${fs::find_up("f")?.sub_string(home.len())} ${fs::find_up("nosuch") ?? "none"}`);
sh::run("cd a/b/c");
print(`find_up: ${fs::find_up("f").sub_string(home.len())} ${fs::find_up("b").sub_string(home.len())} ${fs::find_up("c", "..").sub_string(home.len())}`);
print(`find_up: ${fs::find_up("d", "../../../d/sub").sub_string(home.len())} ${fs::find_up("etc", "/")}`);
P
__luish_internal plugin load ./fs.rhai
echo "load: $?"
[ "$BIN" = "$(printf 'x\377y')" ] && echo "binary contents"
cat > nul.rhai <<'P'
try { fs::exists("a\x00b"); } catch (e) { print(`nul: ${e}`); }
P
__luish_internal plugin load ./nul.rhai
