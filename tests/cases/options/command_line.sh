# On the command line, -o and +o take any option named as for setopt (not
# only dash's names): case and `_` don't matter, and a `no` prefix inverts
# it. --login, --interactive, --stdin, --no-rcs, --no-plugins, --help and
# --version are luish's own.
$SH -o errexit -c 'false; echo not reached'; echo "status $?"
$SH -o ERR_EXIT +o err_exit -c 'false; echo errexit off'
$SH -o no_glob -c 'echo /*'
$SH +o glob -c 'echo /*'
$SH -o noglob +o NoGlob -c 'echo /'
$SH -o prompt_percent -c 'setopt'
$SH -o stdin a b <<'X'
echo "-o stdin: $1 $2"
X
$SH --stdin x y <<'X'
echo "--stdin: $1 $2"
X
$SH -o interactive -c 'case $- in *i*) echo interactive;; esac' </dev/null 2>/dev/null
$SH --interactive -c 'case $- in *i*) echo --interactive;; esac' </dev/null 2>/dev/null
$SH -o bogus -c 'echo not run' 2>/dev/null; echo "status $?"
$SH --errexit -c 'echo not run' 2>/dev/null; echo "status $?"
# --no-rcs skips the startup files: rc.d, $ENV and luishrc, and for a login
# shell login.d (or /etc/profile and ~/.profile).
mkdir -p .config/luish/login.d
echo 'echo luishrc' > .config/luish/luishrc
echo 'echo login.d' > .config/luish/login.d/a.lsh
echo 'echo env' > env.sh
ENV=$HOME/env.sh $SH -i --login -c 'echo run' </dev/null 2>/dev/null
ENV=$HOME/env.sh $SH -i --login --no-rcs -c 'echo run' </dev/null 2>/dev/null
$SH --help | sed -n 1p
$SH --help >/dev/null -c 'echo not run'; echo "status $?"
$SH --version | sed 's/(.*)/(REV)/'
