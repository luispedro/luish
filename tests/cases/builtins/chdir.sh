# chdir is another name for cd, as in dash.
mkdir -p a/b
chdir a/b; pwd
chdir ..; pwd
chdir /nonexistent 2>/dev/null; echo $?
# Options: combined letters, the last of -L and -P wins, -- ends them, and
# - after -- is still the previous directory.
cd "$HOME"
ln -s a/b l
cd -PL l; pwd
cd "$HOME"; cd -LP l; pwd
cd "$HOME"; cd -P -L -- l; pwd
cd -- - >/dev/null; pwd
cd -x 2>/dev/null; echo $?
chdir -q 2>/dev/null; echo $?
pwd
