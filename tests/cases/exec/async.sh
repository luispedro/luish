sleep 0.1 & wait; echo waited $?
(exit 3) & wait $!; echo $?
echo bg > f & wait; cat f
