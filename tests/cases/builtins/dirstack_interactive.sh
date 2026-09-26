# reference: zsh -o noposixcd
# In an interactive shell, pushd and popd print the stack (as in zsh),
# unless -q is given.
mkdir a b c
$SH -i -c 'pushd a; pushd ../b; pushd; pushd +2; popd +1; popd; pushd -q ../c; popd -q; popd +0; (pushd ../b)' </dev/null 2>/dev/null
echo "status $?"
