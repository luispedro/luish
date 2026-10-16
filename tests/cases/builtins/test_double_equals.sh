# reference: zsh
# test and [ accept == for =, as zsh and bash do (an error in dash).
t() { if "$@"; then echo T; else echo F; fi; }
user=nobody
t [ "$user" == nobody ]; t [ "$user" == root ]; t test a == a
t [ ! a == b ]; t [ a == a -a b == c ]; t [ a == b -o c == c ]
t [ == ]; t [ ! == ]; t [ == == == ]; t [ -n == ]; t [ "" == "" ]
