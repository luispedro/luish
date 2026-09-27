# Global aliases (`alias -g`, as in zsh) are expanded in any position, not
# only as command names, but not when quoted, and never as reserved words.
# reference: zsh
unalias -a
alias -g U='| tr a-z A-Z' Y=why SP='echo sp ' X='echo x' L='Y Y'
alias ll='echo ll'
echo abc U
echo Y "Y" 'Y' \Y Y.Y Y=1 aY
X Y
L
for Y in Y; do echo "v=$Y"; done
case Y in why) echo matched-why;; Y) echo matched-Y;; esac
case why in Y) echo pattern-why;; esac
echo out > Y; cat why
echo $(echo Y) `echo Y` "$(echo Y)"
SP ll Y
ll Y
f() { echo Y; }; f
alias -g done=gone
for i in 1; do echo done; done
unalias done
# A global alias's value is expanded once.
alias -g A='B A' B='A B'
echo A B
