# Without a completer for `__luish_internal`, the arguments of its
# subcommands complete as those of the built-ins of the same names:
# variables for `vared` and `where`, built-ins for `help`.
touch ZZfile
ZZone=1
ZZtwo=2
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
c 'vared ZZ'
c '__luish_internal vared ZZ'
c '__luish_internal vared -c ZZ'
c '__luish_internal where ZZ'
c '__luish_internal help ech'
c '__luish_internal savestate ZZ'
