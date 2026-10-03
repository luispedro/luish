# A subscript that doesn't parse, or has no `]` before the `}`, makes a bad
# substitution (as in dash, an error only when expanded), which is read
# again as a word up to the `}`. Nested ones take linear time.
f() { echo ${h['"'}; }
h() { echo ${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[ x}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}; echo "${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[ x}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}"; }
echo ok
echo ${h['"'}
echo never
