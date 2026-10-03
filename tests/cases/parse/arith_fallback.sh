# Deviation from dash: $(( that isn't valid arithmetic is read as a
# command substitution of a subshell (as bash does); dash reports a syntax
# error. From the gnunet-gtk package:
bin="$((which nonexistent-tool ||
echo /opt/gnome/bin/tool)2>/dev/null)"
echo "$bin"
echo $(( echo 1
echo 2
) )
echo bye
# Nested ones take linear time (this took minutes).
echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo $((echo deep) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )) )
