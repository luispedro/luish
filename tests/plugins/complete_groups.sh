# A candidate's `group` is the heading the menu lists it under: the groups
# come in the order of their first candidates (those without one are a group
# without a heading), each with its matches sorted, and of the candidates
# that give the same text the one of the earliest group is kept.
# `__luish_internal complete -g` prints the headings (the menu shows them
# only when there are several groups). Command names are grouped by kind.
cat > frob.rhai <<'R'
sh::completer("frob", |words, i| {
    [
        #{value: "zeta", group: "tags"},
        #{value: "main", group: "branches", desc: "the default"},
        "plain",
        #{value: "alpha", group: "tags"},
        #{value: "main", group: "tags"},
        #{value: "dev", group: "branches"},
        "another",
    ]
});
R
__luish_internal plugin load ./frob.rhai
c() {
    echo "--- $*"
    __luish_internal complete "$@"
    echo "status $?"
}
c -g 'frob '
c 'frob '
c -g 'frob a'
c -g 'frob d'
c -g 'frob nothing'
alias zzcmd=true zzalias=true
zzcmd() { :; }
zzfn() { :; }
c -g 'zz'
