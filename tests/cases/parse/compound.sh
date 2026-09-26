if true; then echo yes; fi
if false; then echo no; elif true; then echo elif; else echo else; fi
if false; then :; else echo else2; fi
for i in 1 2 3; do echo $i; done
for i
do echo never; done
for i in; do echo never; done
i=0; while [ $i -lt 3 ]; do echo w$i; i=$((i+1)); done
until [ $i -eq 0 ]; do echo u$i; i=$((i-1)); done
{ echo brace; echo group; }
(echo subshell)
case abc in
  a*) echo star ;;
  *) echo default ;;
esac
case x in (x) echo paren;; esac
case y in x|y) echo alt;; esac
case z in x) ;; esac
f() { echo func "$@"; }
f a b
g() (echo subshell-func)
g
h()
{
  echo newline-body
}
h
for x in a b; do for y in 1 2; do echo $x$y; done; done
