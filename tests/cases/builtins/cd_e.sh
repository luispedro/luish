# cd -e (POSIX 2024): with -P, status 1 if the new directory's name can't
# be found. dash has no -e.
p() { echo "${PWD#"$HOME"}"; }
mkdir -p a/b
cd -e a; echo $?; p
cd -Pe b; echo $?; p
cd -e -L ..; echo $?; p
# The current directory is removed: cd still changes to it, PWD is the
# logical path, and only -e makes it fail.
mkdir gone
cd gone; rmdir ../gone
cd -P .; echo $?; p
cd -P -e . 2>"$HOME/err"; echo $?; p
[ -s "$HOME/err" ] && echo message
