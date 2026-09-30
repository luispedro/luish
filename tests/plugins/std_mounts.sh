# std's kinds::mount_table: the fields of /proc/mounts with all four of the
# kernel's escapes decoded (a space, tab, newline and backslash), `\134`
# last, so that `\134040` is a backslash followed by `040`.
C=$HOME/.config/luish
mkdir -p "$C" p
printf '[plugins.available]\nstd = { path = "%s" }\n' "$STD_PLUGINS" > "$C/config.toml"
printf '[dependencies]\nstd.completion = "*"\n' > p/plugin.toml
cat > p/extension.rhai <<'X'
import "@std/completion/kinds" as k;
let text = "sysfs /sys sysfs rw 0 0\n/dev/sda1 /media/my\\040disk ext4 rw 0 0\n"
    + "a\\134040b /x\\011y\\012z\\134 fuse.sshfs rw 0 0\nshort line\n";
for m in k::mount_table(text) {
    print(`${m.source}|${m.point}|${m.type}`);
}
// The machine's own.
print(`/proc/mounts: ${k::mount_table().some(|m| m.point == "/")}`);
X
__luish_internal plugin load ./p
echo "load $?"
