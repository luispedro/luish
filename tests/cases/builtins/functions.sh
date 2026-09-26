f() { echo "$0 $1 $#"; }
f a b
g() { return 5; }; g; echo $?
h() { local x=inner; echo $x; }; x=outer; h; echo $x
r() { if [ $1 -le 0 ]; then echo 0; return; fi; echo $1; r $(($1-1)); }; r 3
p() { set -- changed; echo $1; }; set -- orig; p; echo $1
unset -f f; f 2>/dev/null || echo unset-ok
