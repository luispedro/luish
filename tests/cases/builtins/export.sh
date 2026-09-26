export A=1; sh -c 'echo $A'
B=2; sh -c 'echo "[$B]"'; export B; sh -c 'echo $B'
readonly R=ro; echo $R
(R=x) 2>/dev/null; echo $?
unset -v B; echo "[$B]"
export -p | grep -c 'export A='
readonly -p | grep -c R
