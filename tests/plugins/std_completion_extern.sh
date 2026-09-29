# Another plugin reusing std's completion engine: `@std/completion/lib`
# completes from its own specs, whose kinds and sub_spec name modules of
# that plugin as `@SOURCE/PLUGIN/MODULE:NAME`. Those modules' functions
# call their own helpers and import their neighbours (and std's kinds).
C=$HOME/.config/luish
B=extra/bio
mkdir -p "$C" "$B/specs" data/sub
printf '[plugins.available]\nstd = { path = "%s" }\n' "$STD_PLUGINS" > "$C/config.toml"
printf '[dependencies]\nstd.completion = "*"\n' > "$B/plugin.toml"
cat > "$B/extension.rhai" <<'X'
sh::completer("seqtool", |words, i| {
    import "@std/completion/lib" as lib;
    import "specs/seqtool" as s;
    lib::complete(s::spec(), words, i)
});
X
cat > "$B/specs/seqtool.rhai" <<'X'
fn spec() {
    #{
        opts: "--format=FMT  output format\n-v, --verbose  say more",
        common: "-r, --ref FILE  the reference",
        values: #{"--ref": "@extra/bio/kinds:fasta", "--format": "@extra/bio/kinds:formats"},
        commands: "view  view a file\nfaidx, index  index a FASTA file",
        sub_spec: "@extra/bio/specs/seqtool:sub",
        skip: #{"+": "@extra/bio/kinds:formats"},
    }
}
fn sub_spec(name, sub) {
    switch sub {
        "view" => #{opts: "-H  the header only", args: ["@extra/bio/kinds:fasta", "@extra/bio/kinds:regions"]},
        "faidx" => #{opts: "-o FILE  output", args: ["@extra/bio/kinds:fasta"], args_if: #{"-o": ["dirs"]}},
        _ => (),
    }
}
X
cat > "$B/kinds.rhai" <<'X'
import "formats" as formats;
import "@std/completion/kinds" as std_kinds;
fn fasta_suffixes() {
    [".fa", ".fasta", ".fna"]
}
fn is_fasta(name) {
    fasta_suffixes().some(|s| name.ends_with(s))
}
// FASTA files and directories.
fn fasta(cur) {
    let r = std_kinds::listing(cur, "", false);
    r.candidates = r.candidates.filter(|c| type_of(c) == "map" || is_fasta(c));
    r
}
// The sequences of the FASTA file before, from its .fai.
fn regions(words) {
    for w in words {
        if is_fasta(w) {
            let fai = fs::read_file(w + ".fai") ?? "";
            return fai.split("\n").filter(|l| l != "").map(|l| l.split("\t")[0]);
        }
    }
    []
}
fn kind(name, cur, words) {
    switch name {
        "fasta" => fasta(cur),
        "formats" => formats::all(),
        "regions" => regions(words),
        _ => throw `unknown kind: ${name}`,
    }
}
X
echo 'fn all() { [#{value: "fasta", desc: "sequences"}, #{value: "sam", desc: "alignments"}] }' > "$B/formats.rhai"
touch data/genome.fa data/reads.fastq data/other.fna
printf 'chr1\t100\t6\t60\t61\nchr2\t50\t120\t60\t61\n' > data/genome.fa.fai
c() {
    echo "--- $1"
    __luish_internal complete "$1"
}
__luish_internal plugin load ./extra/bio
echo "load $?"
c 'seqtool --ref data/'
c 'seqtool -r data/g'
c 'seqtool --format='
c 'seqtool +'
c 'seqtool '
c 'seqtool -v view data/'
c 'seqtool view data/genome.fa '
c 'seqtool index data/'
c 'seqtool faidx -o data/genome.fa d'
c 'seqtool faidx --ref=data/o'
