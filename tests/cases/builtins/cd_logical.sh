# cd and pwd use the logical directory kept by the shell (dash's curdir):
# pwd works after the directory is removed, cd - without OLDPWD is cd .,
# and OLDPWD is exported.
here=$(pwd)
unset OLDPWD
cd - >/dev/null; echo "status=$?"
cd /
cd "$here"
echo "old: $OLDPWD"
env | grep '^OLDPWD='
cd - ; cd "$here"
mkdir -p gone/sub
cd gone/sub
rmdir "$here/gone/sub"
pwd | sed "s|^$here|HERE|"; echo "status=$?"
cd "$here"
echo "${OLDPWD##*/}"
cd gone; cd ..; pwd | sed "s|^$here|HERE|"
PWD=/nonsense; pwd | sed "s|^$here|HERE|"
ln -s gone link
cd link; pwd | sed "s|^$here|HERE|"; cd -P .; pwd | sed "s|^$here|HERE|"
