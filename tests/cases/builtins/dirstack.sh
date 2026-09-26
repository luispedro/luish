# reference: zsh -o noposixcd
# pushd, popd and dirs follow zsh, with POSIX_CD off as by default (zsh's
# sh emulation turns it on, which makes +n and -n directory names).
p() { echo "$1 ${PWD#"$HOME"}"; }
mkdir a b c gone ./-d
ln -s a link
pushd a; p $?
echo "OLDPWD ${OLDPWD#"$HOME"}"
pushd ../b; p $?
dirs; dirs -v; dirs -p; dirs -l | sed "s|$HOME|H|g"; dirs -pv
# Without a directory: swap the top two entries.
pushd; p $?; dirs
# +n and -n rotate the stack.
pushd +2; p $?; dirs
pushd -0; p $?; dirs
pushd -1; p $?; dirs
pushd +0; p $?; dirs
pushd +3; p $?; dirs
pushd -3; p $?; dirs
pushd +99999999999999999999999; p $?; dirs
pushd -q c; p $?; dirs
# Only q, L and P are options; anything else is the operand.
dirs -c; cd
pushd -q -d; p $?; popd -q
pushd -q -- -d; p $?; popd -q
pushd -x; p $?
pushd a b; p $?
dirs
# popd +n and -n remove an entry without changing directory.
cd; dirs a b c
popd +1; p $?; dirs
popd -1; p $?; dirs
popd -2; p $?; dirs
popd -0; p $?; dirs
popd; p $?; dirs
popd; p $?; dirs
popd +1; p $?; dirs
# Entry 0 is the current directory: removing it is popd.
pushd -q a; pushd -q ../b; popd +0; p $?; dirs
popd -1; p $?; dirs
popd -0; p $?; dirs
# pushd - goes to OLDPWD; an empty stack makes pushd go to HOME.
cd a; cd ..
pushd -; p $?; dirs
popd -q; pushd; p $?; dirs
dirs -c; dirs
cd a; pushd; p $?; dirs; dirs -c
# A directory that was removed: pushd fails and leaves the stack alone,
# popd removes it anyway.
pushd -q gone; pushd -q ..; rmdir gone; dirs
pushd +1; p $?; dirs
popd; p $?; dirs
dirs -c
# -P resolves symbolic links, as for cd.
pushd -q link; p $?
pushd -qP ../link; p $?
pushd -q -P -L ../link; p $?
dirs -c
# The stack is copied into subshells.
cd; pushd -q a; (pushd -q ../b; dirs); dirs
# Bad options.
dirs -x; echo $?
dirs -c
