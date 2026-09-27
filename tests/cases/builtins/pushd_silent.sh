# reference: zsh -o noposixcd
# setopt pushd_silent: an interactive shell doesn't print the stack after
# pushd and popd; auto_pushd's cd never prints it.
mkdir a b
$SH -i -c 'setopt autopushd; cd a; cd ../b; pushd ../a; setopt pushd_silent; pushd ../b; popd; popd +1; dirs' </dev/null 2>/dev/null
echo "status $?"
