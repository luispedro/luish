# A quoted expansion that yields nothing is still one (empty) field.
set -- "$u"; echo $#
set -- "${u+set}"; echo $#
set -- "${u-}"; echo $#
set -- "${u:+x}"; echo $#
set -- "$(true)"; echo $#
test "${PATH_SEPARATOR+set}" != set && echo unset
set --; set -- "$@"; echo $#
set --; set -- "$@"""; echo $#
set --; set -- "$u$@"; echo $#
set --; set -- "$*"; echo $#
