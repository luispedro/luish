x=$(case a in a) echo matched;; esac)
echo "$x"
echo $( (echo subshell-in-cmdsubst) )
echo $((1 + (2 * 3)))
echo "$(echo a; echo b)"
echo $(echo "$(echo deep)")
