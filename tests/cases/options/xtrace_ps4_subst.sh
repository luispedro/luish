# A command substitution in PS4 is not traced itself (dash's inps4), so
# set -x doesn't loop.
$SH -c 'set -x; PS4="+\$(echo t) "; echo hi; x=$(echo y); echo "$x"' 2>/dev/null
( set -x; PS4='+$(echo t) '; echo sub ) 2>/dev/null
