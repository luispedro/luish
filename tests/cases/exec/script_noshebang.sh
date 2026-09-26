printf 'echo from script $1\nexit 4\n' > s
chmod +x s
./s arg; echo $?
