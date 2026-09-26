umask 022; umask; umask -S
umask u=rwx,g=,o=; umask
( ulimit -n 64; ulimit -n )
times >/dev/null; echo times $?
hash sh; echo hash $?
pwd -P >/dev/null; echo pwd $?
