# Line continuations are removed inside $ expansions, as in dash.
ab=1; a=2
echo $\
?
echo $a\
b
echo ${\
ab}
echo ${a\
b}
echo $\
\
{ab}
echo $(\
(1+2))
echo $\
(echo x)
echo "$\
?" "${#\
ab}"
printf %s "echo \$\\
?
" | $SH
