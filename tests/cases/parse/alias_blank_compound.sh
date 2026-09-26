# After an alias ending in a blank, the next word is checked for aliases
# even as a for variable, `in`, a for word, or a case word (as in dash).
alias FOR1='for '
alias FOR2='FOR1 '
alias eye1='i '
alias eye2='eye1 '
alias IN='in '
alias onetwo='$one "2" '
one=1
FOR2 eye2 IN onetwo 3; do echo $i; done
alias e_='for i in 1 2 3; do echo $i;'
e_ done
alias e2='for i in 1 2 3; do echo '
e2 $i; done
alias CASE='case '
alias word='x '
CASE word IN x) echo matched;; esac
