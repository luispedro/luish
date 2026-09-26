x=hello; empty=; unset unset
echo ${x} ${#x} ${x:-d} ${empty:-d} ${unset-d} ${empty-d}
echo ${x:+alt} ${empty:+alt} ${empty+set} ${unset+set}
echo ${unset2=assigned} $unset2 ${empty:=filled} $empty
path=/a/b/c.tar.gz
echo ${path#*/} ${path##*/} ${path%.*} ${path%%.*}
echo ${path#/a} ${path%nomatch}
echo "${x#h}" "${x%"lo"}" ${x#"h*"}
set -- one two three
echo $1 $2 ${3} $# ${10-none}
echo "${#}" ${#1}
star='*'; echo "${x%$star}" ${x%"$star"}
