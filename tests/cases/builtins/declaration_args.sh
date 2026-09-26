# Arguments of export, readonly and local that look like assignments are
# expanded as assignments (dash 0.5.12): no field splitting or globbing,
# and tilde expansion after = and :. Also through `command` and when the
# command name comes from an expansion.
HOME=/home/bob
x='a b'
touch f1 f2
f() {
  local foo=$1 y=~/s z=foo:~ w=*
  echo "$foo|$y|$z|$w"
}
f 'void *'
readonly c=~/src
echo $c
z=command
$z readonly d=$x
echo "$d"
\command export e=$x
echo "$e"
command -p export g=$x:~
echo "$g"
export "h=$x" 2>/dev/null
echo "$h"
export i=* j
echo "$i"
export k=$x l=$x
echo "$k|$l"
set -- m=1 n=2
export "$@"
echo "$m $n"
command -v export
# Tilde expansion in ${x-word} within an assignment, after ':' too.
y=${undef-~:~} v=~:${undef-a:~}
echo "$y $v"
echo ${undef-~} "${undef-~}" ${undef-a:~}
