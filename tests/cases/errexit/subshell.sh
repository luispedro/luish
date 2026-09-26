set -e
x=$(false; echo still)
echo "x=$x"
( false; echo not-in-sub )
echo not-reached
