# `shopt` (as in bash) sets the luish setting that does the same, as
# `setopt` does: `globstar` is `glob.star`, `autocd` is `cd.auto`.
shopt -s globstar autocd; echo "status $?"; setopt
shopt globstar autocd; echo "status $?"
shopt -u globstar; shopt globstar autocd; echo "status $?"
shopt -p globstar autocd
shopt -q autocd; echo "status $?"
shopt -q globstar; echo "status $?"
# Options that luish always has can be set, and those it never has unset.
shopt -s histappend checkwinsize; echo "status $?"
shopt -u extglob nullglob; echo "status $?"
shopt -s extglob 2>&1; echo "status $?"
shopt -u xpg_echo 2>&1; echo "status $?"
# Errors.
shopt -su autocd 2>&1; echo "status $?"
shopt -s bogus autocd 2>&1; echo "status $?"
shopt bogus 2>&1; echo "status $?"
shopt -z 2>&1; echo "status $?"
# `-o`: the options of `set -o`.
shopt -so pipefail; set -o | grep pipefail
shopt -po errexit pipefail
shopt -o pipefail bogus 2>&1; echo "status $?"
shopt -uo pipefail; shopt -o pipefail; echo "status $?"
# Listings: all options, those on, and as commands that read back.
shopt
shopt -s
shopt -u -p | head -3
shopt -p >saved; shopt -s globstar; . ./saved; shopt globstar; echo "status $?"
