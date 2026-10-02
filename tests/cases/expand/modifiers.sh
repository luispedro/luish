# reference: zsh
# zsh's modifiers in parameter expansion: ${x:h}, ${x:t}, ${x:r}, ${x:e},
# ${x:a}, ${x:A}, ${x:u}, ${x:l}, counts after h and t, and several in a
# row. On a list, they apply to each element, or to the joined string in
# "${a[*]...}" and "${*...}", as ${x/a/b} does.
x=/usr/lib/foo.tar.gz
echo "${x:h} ${x:t} ${x:r} ${x:e} ${x:u}"
echo "${x:h:h} ${x:t:r} ${x:t:r:r} ${x:r:e} ${x:h:t}"
echo "${x:h1} ${x:h2} ${x:h9} ${x:t2} ${x:t9} ${x:h0} ${x:t0}"
X=ABC.Def; echo "${X:l} ${X:l:e}"
for p in '' / // /a/ a/ a//b a///b/// //a ///a .bashrc a.b/c foo. a.b.c /x/.y.z ../a .. . a/.. 'a b/c d.e'; do
  printf '[%s]' "$p" "${p:h}" "${p:t}" "${p:r}" "${p:e}" "${p:h1}" "${p:t2}"
  echo
done
# Without braces, :h is text.
y=a/b; echo $y:h "$y:t"
# An unset variable is empty.
echo "[${unset:h}] [${unset:t}] [${unset:a}]"
# Lists.
a=(/a/b.c /d/e.f)
set -- /p/q.x /r/s.y
printf '[%s]' "${a[@]:t}" "${a[*]:t}" ${a[*]:t} "${a:t}" "${a[1]:t}" "${@:h}" "${*:t}" ${*:t}; echo
printf '[%s]' "${a[@]:t:r}" "${a[@]:e}"; echo
# With flags.
printf '[%s]' "${(U)a[@]:t}" "${(j:,:)a[@]:h}"; echo
# Inside other expansions.
echo "${u:-${x:t}}" "$(echo "${x:h}")"
# :a and :A, shown relative to the physical current directory.
mkdir -p real/sub
touch real/f.txt
ln -s real lnk
ln -s nowhere dangling
ln -s .. real/sub/up
here=$(pwd -P)
for p in lnk/f.txt lnk/../x real/sub/up/f.txt real/./sub/../f.txt real//sub/ dangling dangling/x nonexist/x/../y \
  lnk/nonexist/../f.txt; do
  a=${p:a} A=${p:A}
  echo "$p: [${a#"$here"}] [${A#"$here"}] ${p:A:t}"
done
echo "${here:a:h:h:A:t}" | grep -q . && echo ok
p=/; echo "${p:a} ${p:A} ${p:A:h}"
cd lnk
f=f.txt; a=${f:a} A=${f:A}
echo "${a#"$here"} ${A#"$here"}"
