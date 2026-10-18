# `id -u`, `id -g` (with `-n`, `-r`) and `uname` with `-s -n -r -v -m` run
# in the shell when they are the system's (`exec/utility.rs`), and in
# `$(...)` without a subshell; anything else runs the utility. Output and
# status must be the utility's.

ti() {
    x=$(id "$@" 2>/dev/null); s=$?
    printf 'id %s => [%s] %s | ' "$*" "$x" "$s"
    id "$@" > out 2>/dev/null; echo "$? [$(cat out)]"
}
tu() {
    x=$(uname "$@" 2>/dev/null); s=$?
    printf 'uname %s => [%s] %s | ' "$*" "$x" "$s"
    uname "$@" > out 2>/dev/null; echo "$? [$(cat out)]"
}
for a in -u -g -un -gn -nu '-u -n' '-n -u' -ur -ru '-u -r' -urn '-r -g -n' -uu \
    '' -G -Gn -n -r -rn -ug -- '-- -u' '-u root' '-un root' -uz --user; do
    ti $a
done
for a in '' -s -n -r -v -m -sm -ms '-m -s' -snrvm '-v -r' '-s -s' -smm \
    -a -o -p -i -- '-- -s' --kernel-name x -sx -; do
    tu $a
done
u=$(id -u); n=$(id -un); [ "$u" -ge 0 ] && [ -n "$n" ] && echo ok
case $(uname) in Linux) echo linux ;; *) echo other ;; esac
id -u | cat
uname -s | cat
