# sh::glob(pattern, dir): the shell's patterns with fixed options (braces,
# qualifiers, **/, no matches giving nothing, hidden files only with D or
# a leading .), matched in a literal directory and relative to it.
mkdir -p 'd/sub/deep' 'd/[x]' d/__pycache__
touch d/a.fa d/b.fa.gz d/c.txt d/.hidden.fa d/sub/x.fa d/sub/deep/y.fa 'd/{lit}.fa' 'd/[x]/z.fa'
ln -s sub d/link
ln -s nowhere d/broken
ln -s a.fa d/afa-link
cat > g.rhai <<'P'
for t in [
    ["*", "d"], ["*(/)", "d"], ["*(-/)", "d"], ["*(-/M)", "d"], ["*(.)", "d"], ["*(-.)", "d"], ["*(@)", "d"],
    ["*(^/)", "d"], ["*(T)", "d"], ["*(D)", "d"], [".*", "d"], ["*.{fa,fasta}{,.gz}", "d"], ["*.{fa,fa}", "d"],
    ["*/*.fa", "d"], ["**/*.fa", "d"], ["**/*.fa(:t)", "d"], ["*.fa(:r)", "d"], ["\\{lit\\}.fa", "d"],
    ["{lit}.fa", "d"], ["*", "nowhere"], ["sub", "d"], ["sub(.)", "d"], ["missing", "d"], ["d/*.fa", ""],
    ["*.fa", "d/"], ["*.fa", "d/[x]"], ["*(N)", "d/sub/deep"], ["*(On[0])", "d/sub"], ["*(on)", "d/sub"],
] {
    print(`${t[0]} in ${t[1]}: ${sh::glob(t[0], t[1])}`);
}
print(sh::glob("d/*(/)"));
try {
    sh::glob("*(Z)", "d");
} catch (e) {
    print(e);
}
P
__luish_internal plugin load ./g.rhai
echo "load: $?"
# The shell gives the same, but sorts each brace's alternative on its own.
setopt glob.star glob.bare_qualifiers expand.braces
cd d && echo *(-/M) **/*.fa *.{fa,fasta}{,.gz}
