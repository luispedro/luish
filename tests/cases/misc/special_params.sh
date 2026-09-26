echo $0
sh -c 'echo $0 $1' arg0 arg1
echo $$ | grep -c '^[0-9][0-9]*$'
sleep 0 & [ "$!" -gt 0 ] && echo bgpid; wait
true; echo $?
