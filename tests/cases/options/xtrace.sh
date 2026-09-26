exec 2>&1
set -x
echo hi 'a b' ""
x=1 y='two words'
f() { echo in-f; }
f
set +x
echo off
