# bash's FUNCNAME, BASH_LINENO and caller, with BASH_SOURCE: a frame for the
# script, each file read with `.` and each function call, innermost first.
# FUNCNAME is set only while a function runs; its bottom is `main` in a
# script, and a file read with `.` is `source`. BASH_LINENO is the line each
# was called from (0 for the script).
show() {
  echo "$1: FUNCNAME=[${FUNCNAME[*]}] BASH_LINENO=[${BASH_LINENO[*]}] BASH_SOURCE=[${BASH_SOURCE[*]}]"
  echo "  caller=[$(caller)] 0=[$(caller 0)] 1=[$(caller 1)] 9=[$(caller 9)]"
}
show top
cat > lib.sh <<'X'
show lib
libf() {
  show libf
}
X
. ./lib.sh
f() {
  libf
}
f
echo "f: [$FUNCNAME] [${FUNCNAME-unset}]"
g() { echo "g: $FUNCNAME ${#FUNCNAME[@]} $BASH_LINENO"; . ./src.sh; }
echo 'echo "src: [${FUNCNAME[*]}] [${BASH_LINENO[*]}]"; caller' > src.sh
g
. ./src.sh
echo "top: [${FUNCNAME-unset}] [${BASH_LINENO[*]}] $(caller) $?"
# In -c: no frame at the top level (no output, status 1), and a function's
# caller has no file.
$SH -c 'caller; echo "status $?"; echo "[${FUNCNAME-unset}] [${BASH_LINENO-unset}]"
h() { echo "h: [${FUNCNAME[*]}] [${BASH_LINENO[*]}] [${BASH_SOURCE[*]}]"; caller; caller 0; echo "status $?"; }
h'
# Errors.
caller x; echo "status $?"
caller 1 2; echo "status $?"
caller -1; echo "status $?"
# Assigning makes it an ordinary variable, as for the other specials.
(FUNCNAME=mine; f2() { echo "$FUNCNAME"; }; f2)
typeset -p BASH_LINENO
