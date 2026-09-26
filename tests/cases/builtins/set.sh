set -- a b c; echo $# $1
shift; echo $# $1
shift 2; echo $#
set -- x; set --; echo $#
set -f; echo "$-" | grep -c f; set +f
set -u; (echo $undefined_var) 2>/dev/null; echo "status $?"; set +u
set -o noglob; echo *; set +o noglob
set -- 1 2 3; shift 5 2>/dev/null; echo never
