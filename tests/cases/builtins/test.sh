t() { if "$@"; then echo T; else echo F; fi; }
t test; t test ""; t test x; t test -n ""; t test -z ""
t [ a = a ]; t [ a != a ]; t [ 1 -eq 1 ]; t [ 2 -gt 10 ]; t [ -5 -lt 3 ]
t [ ! a = b ]; t [ a = b -o c = c ]; t [ a = a -a b = c ]
t [ \( a = a \) ]; t [ -d / ]; t [ -f / ]; t [ -e /nonexistent ]
touch f; t [ -f f ]; t [ -s f ]; t [ -r f ]; t [ -x f ]
t [ "" ]; t [ ! ]; t [ -n ]; t [ = ]; t [ a \< b ]
t test ! -z x; t test -h /nonexistent
test 1 -eq x 2>/dev/null; echo $?
[ 1 = 1 2>/dev/null; echo $?
