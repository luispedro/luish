# set, export, readonly, alias and trap quote values as dash's single_quote
# does ('"'"' for a single quote).
a=1 b= c="'" d="''x'" e='x y' f="a'b" g="'a" h="a'"
set | grep -E '^[a-h]='
export f; export -p | grep ' f='
readonly r="x'y"; readonly -p | grep ' r='
alias q="it's" z="''"; alias q z
trap "echo 'bye'" EXIT; trap; trap - EXIT
