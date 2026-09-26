# setopt globstar: `**/` matches any number of directories, as in zsh
# (where it is always on). Hidden directories and symbolic links to
# directories are skipped; `***/` follows the links.
# reference: zsh
mkdir -p a/b/c .hid/sub e
touch x.md a/y.md a/b/z.md a/b/c/w.md .hid/h.md .hid/sub/s.md a/.dot.md
ln -s a lnk
[ -n "$ZSH_VERSION" ] || setopt globstar
echo 1 **/*.md
echo 2 a/**/*.md
echo 3 **/
echo 4 a/**/
echo 5 **/c
echo 6 a/**
echo 7 x**/*.md
echo 8 "**"/*.md '**'/*.md
echo 9 **/nomatch
echo 10 nodir/**/*.md
echo 11 **/b/**/*.md
echo 12 ***/*.md
echo 13 .h*/**/*.md
echo 14 /nonexistent-luish-dir/**/x
cd a/b; echo 15 ../**/*.md; cd ../..
for f in **/*.md; do printf '%s;' "$f"; done; echo
