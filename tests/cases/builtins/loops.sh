for i in 1 2 3; do if [ $i = 2 ]; then continue; fi; echo $i; done
for i in 1 2 3; do for j in a b; do [ $j = b ] && break 2; echo $i$j; done; done
i=0; while :; do i=$((i+1)); [ $i -gt 3 ] && break; echo $i; done
for i in x y; do while true; do continue 2; done; done; echo done
