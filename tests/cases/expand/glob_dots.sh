# setopt glob.dots (zsh's GLOB_DOTS, bash's dotglob): patterns match names
# with a leading `.` (but not `.` and `..`), and `**/` enters hidden
# directories.
# reference: zsh
mkdir -p t/.hd/s t/d; cd t
touch a .h d/.x .hd/f .hd/s/f
setopt GLOB_DOTS
echo 1 *
echo 2 d/*
echo 3 */
echo 4 *h* ?h
[ -n "$ZSH_NAME" ] || setopt glob.star
echo 5 **/f
unsetopt GLOB_DOTS
echo 6 * **/f
