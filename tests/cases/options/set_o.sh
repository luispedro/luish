# set -o and set +o list the options, also when they end the arguments.
# (dash's last option is `debug`; luish's are `pipefail` and `hashall`, which
# Debian's dash lacks: see set_o_hashall.sh.)
set -o | grep -v -e debug -e hashall -e pipefail
set +o | grep -v -e debug -e hashall -e pipefail
(set -o) | head -2
x=`(set -o) 2>/dev/null`
echo "${x%%
*}"
set -e -o | grep errexit
set -eo | grep errexit; echo $?
