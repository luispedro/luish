# The completion plugin of luish-std-plugins for luish's own built-ins:
# their options, and style's names, values and colour schemes. The other
# arguments of built-ins that luish completes itself (cd's directories,
# unset's variables and functions) still come from luish.
__luish_internal plugin load "$STD_PLUGINS/completion"
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
mkdir -p src scripts
touch notes.txt
f() { :; }
echo "=== style"
c 'style -'
c 'style -c '
c 'style -c default-dark default-l'
c 'style -c a b c '
c 'style --terminal-colors '
c 'style command.a'
c 'style comment it'
c 'style comment no-b'
c 'style comment bold bg:bright-r'
c "style comment 'bold ul:re"
c 'style -r comm'
c 'style -d -'
c 'style -d -r command.u'
c 'style -s default-dark -'
c 'style -s default-dark -i '
c 'style -s default-dark command.f'
c 'style -s default-dark terminal.b'
c 'style -s default-dark terminal.background '
c 'style --clear '
echo "=== __luish_internal"
c '__luish_internal st'
c '__luish_internal style -c default-d'
c '__luish_internal check-cache '
c '__luish_internal print -N'
echo "=== options, and luish's own arguments"
c 'cd -'
c 'cd s'
c 'unset -'
c 'unset -f '
c 'typeset -'
c 'set -o pipe'
c 'ulimit -'
c 'ulimit -n '
c '[ -n'
c '[ -f n'
c 'test -e'
c 'print -'
c 'echo -'
c 'pwd '
c 'dirs -'
