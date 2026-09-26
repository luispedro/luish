# popd with an argument other than +n or -n is an error. zsh does something
# odd with a directory there (usually nothing, with status 0).
mkdir a
pushd -q a
popd a; echo $?
popd +x; echo $?
dirs
