# reference: zsh -o noposixcd
# setopt auto_pushd: cd pushes the old directory, as pushd does, and takes
# +n and -n to take an entry out of the stack.
mkdir a b c
setopt autopushd
cd a; cd ../b; cd ../a; cd ../c
dirs -v
cd - >/dev/null; dirs
cd +2; dirs
cd -1; dirs
cd +9 2>/dev/null; echo "status $?"; dirs
cd; dirs
cd nosuch 2>/dev/null || echo failed; dirs
cd . ; dirs
# pushd_ignore_dups: the new directory is taken out of the stack, also
# after pushd, popd, and cd without auto_pushd.
dirs -c
setopt pushd_ignore_dups
cd a; cd ../b; cd ../a; dirs
pushd ../c; dirs
pushd +2; dirs
popd; dirs
unsetopt auto_pushd
cd ../b; dirs
