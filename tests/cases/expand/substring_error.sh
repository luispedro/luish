# reference: zsh
# A negative length that ends before the offset is an error.
x=abcdef
echo ${x:3:-3}.
echo ${x:3:-4}
echo never
