show() { echo "$#:" "$@"; for a; do printf '<%s>' "$a"; done; echo; }
set --
show "$@"; show $@; show "$*"; show $*
set -- a "b c" "" d
show "$@"; show $@; show "$*"; show $*
show "x$@y"; show "${@}"
IFS=:; show "$*"; show $*; IFS=; show "$*"; unset IFS; show "$*"
set -- "  lead" "trail  "
show $@
