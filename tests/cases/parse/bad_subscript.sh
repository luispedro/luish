# A subscript that doesn't parse, or has no `]` before the `}`, makes a bad
# substitution (as in dash, an error only when expanded), which is read
# again as a word up to the `}`. Nested ones take linear time.
f() { echo ${h['"'}; }
# Where the subscript ends is where it ends read unquoted (as in zsh):
# the `"` doesn't hide the `}`.
k() { echo ${h['"'}
#"
echo "]"
}
h() { echo ${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[ x}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}; echo "${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[${a[ x}}}}}}}}}}}}}}}}}}}}}}}}}}}}}}"; }
echo ok
echo ${h['"'}
echo never
