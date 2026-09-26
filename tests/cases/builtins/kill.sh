sleep 10 & p=$!
kill $p; wait $p; echo $?
kill -l 15; kill -l 143
sleep 10 & kill -s KILL $!; wait $!; echo $?
