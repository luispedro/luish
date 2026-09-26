mkdir -p a/b
cd a/b; pwd
cd ..; pwd
cd -; cd - >/dev/null; pwd
cd /; pwd
cd /nonexistent 2>/dev/null; echo $?
cd; echo "$PWD" | grep -c test
