# The completion plugin of luish-std-plugins (std/completion): options,
# their values, and the arguments of common commands, as Tab offers them.
__luish_internal plugin load "$STD_PLUGINS/completion"
echo "load $?"
__luish_internal plugin list-loaded
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
mkdir -p src/deep man/man1 man/man3 other .ssh/conf.d
touch notes.txt src/main.c src/deep/x.c ./-dash
touch man/man1/ls.1.gz man/man1/lsblk.8 man/man1/printf.1 man/man3/printf.3.gz
echo "=== options"
c 'ls --colo'
c 'ls --color='
c 'ls --color=al'
c 'cat -'
c 'cat --show-e'
c 'cat -Anv'
c 'rm -rf'
c 'cp --t'
c 'cp --no-'
c 'egrep --colo'
c 'ls --nosuch='
echo "=== values, as the next word, after = and in a word of options"
c 'cp -t s'
c 'cp --target-directory=s'
c 'cp -vts'
c 'sort --sort=n'
c 'tail --follow='
c 'mkdir -m 75'
c 'tar -xf n'
c 'tar -xfn'
c 'tar --format=p'
c 'wc --total=o'
echo "=== the end of options"
c 'cat -- -'
echo "=== arguments"
c 'mkdir s'
c 'rmdir s'
c 'chmod 75'
c 'chmod +x n'
c 'chmod --reference=notes.txt n'
c 'chown roo'
c 'chown root:roo'
c 'chgrp roo'
c 'grep ma'
c 'grep -e ma n'
c 'grep --regexp=ma n'
c 'tr [:up'
c 'uniq notes.txt out '
c 'date +'
echo "=== dd"
c 'dd st'
c 'dd if=n'
c 'dd conv=notrunc,sy'
echo "=== make"
printf '%s\n' 'all: build' 'build test: x' '.PHONY: all' '%.o: %.c' 'CC := gcc' 'install:' '	echo x: y' > Makefile
printf '%s\n' 'deploy:' > other/GNUmakefile
c 'make '
c 'make te'
c 'make -C other '
c 'make --directory=o'
c 'make -j'
echo "=== man"
MANPATH=$HOME/man
c 'man ls'
c 'man pri'
c 'man 3 pri'
c 'man 3 ls'
c 'man -l n'
echo "=== ssh"
cat > .ssh/config <<'E'
Host alpha beta
    User me
Host *.corp !gamma
Include conf.d/*.conf conf.d/x[0-9]?.cf
E
echo 'host delta' > .ssh/conf.d/one.conf
echo 'Host no_pe' > .ssh/conf.d/one.other
echo 'Host no_hidden' > .ssh/conf.d/.hidden.conf
echo 'Host epsilon' > .ssh/conf.d/x1a.cf
echo 'Host no_letter' > .ssh/conf.d/xaa.cf
c 'ssh al'
c 'ssh me@be'
c 'ssh -J del'
c 'ssh no_p'
c 'ssh eps'
c 'ssh no_h'
c 'ssh no_l'
c 'ssh !'
c 'ssh -o Strict'
c 'ssh -l roo'
c 'ssh -e'
c 'luish --ssh al'
c 'luish --ssh -p 22 me@be'
c 'luish --ssh -l roo'
c 'luish --ssh -o Strict'
c 'luish --ssh -o ssh.'
c 'luish --ssh -e'
c 'luish --ssh --'
c 'luish --ssh alpha '
c 'luish --remote ssh -T del'
c 'luish --remote ssh -e'
c 'luish --remote no_such_command no'
c 'scp n'
c 'scp de'
c 'scp me@de'
c 'scp delta:no'
c 'rsync -av --del'
c 'rsync -a de'
echo "=== tar"
tar -cf a.tar src notes.txt
c 'tar -xf a.tar '
c 'tar -xf a.tar src/'
c 'tar xf a.tar n'
c 'tar -cf b.tar s'
echo "=== processes"
c 'pkill --signal=KI'
c 'killall -s TE'
echo "=== git"
c 'git swi'
