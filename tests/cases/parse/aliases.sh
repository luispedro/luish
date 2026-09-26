alias say='echo said'
say hello
alias ll='echo ll ' x='echo xx'
ll x
unalias say
alias x ll
alias say 2>/dev/null || echo not-found
