type echo cd if
f() { :; }; type f
command -v echo; command -v nonexist_x; echo $?
command -V true
alias q=quux; command -v q
command echo via-command
