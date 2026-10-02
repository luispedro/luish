# Errors with arrays: the status is 2, as for other expansion and readonly
# errors in dash (zsh uses 1).
set -u
a=(x)
echo "${a[@]}" "${a[0]}"
(echo "${a[5]}"; echo never)
echo "status $?"
(echo "${u[@]}"; echo never)
echo "status $?"
set +u
readonly r=(1 2)
(r[0]=3; echo never)
echo "status $?"
(r+=(3); echo never)
echo "status $?"
(r=(3); echo never)
echo "status $?"
echo "${r[@]}"
# An index past the limit (64 Mi elements) is an error, not an
# allocation that fails.
(a[133333333332]=3; echo never)
echo "status $?"
(: $((a[67108864]=3)); echo never)
echo "status $?"
(a=([133333333332]=3); echo never)
echo "status $?"
a[1000]=y
echo "${#a[@]}"
