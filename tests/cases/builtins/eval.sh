eval echo hello
x='echo $y'; y=value; eval $x
eval 'a=1; b=2'; echo $a $b
eval "f() { echo defined; }"; f
eval 'exit 3'
