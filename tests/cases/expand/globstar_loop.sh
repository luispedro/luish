# `***/` stops at a symbolic link to a directory it is already in (zsh
# follows it until the path is too long, with an error for each loop).
mkdir -p a/b
touch a/b/x
ln -s .. a/b/up
setopt globstar
echo ***/x
echo **/x
