x=
(echo ${x:?}) 2>/dev/null; echo $?
(echo ${y?}) 2>/dev/null; echo $?
echo after
