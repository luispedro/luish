# Without `setopt globstar`, `**` is the same as `*` (as in dash), and
# `(` can't follow a pattern.
mkdir -p a/b/c
touch x.md a/y.md a/b/z.md a/b/c/w.md
echo **/*.md
echo a/**
echo ***/*.md
echo **/
$SH -c 'echo *(/)' 2>/dev/null; echo "status $?"
