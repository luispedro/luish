# Without `setopt syntax.repeat`, `repeat` is an ordinary word, as in dash:
# a command, a function or an argument of one.
repeat() {
	echo "function $*"
}
repeat 2 arg
repeat 3
x=repeat
echo "$x" repeat
case repeat in repeat) echo pattern ;; esac
# A brace group still needs its `;`, so these are arguments and a group.
echo 2 { echo brace }
unset -f repeat
command -v repeat
echo "status $?"
