printf 'echo sourced $1\nv=set\nreturn 2\necho never\n' > inc.sh
. ./inc.sh; echo "$? $v"
