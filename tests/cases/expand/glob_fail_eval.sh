# glob.fail in `eval`: as for other expansion errors (as in dash), an
# unmatched pattern exits a non-interactive shell. zsh and bash return 1
# from `eval` instead.
setopt glob.fail
(eval 'echo x*'; echo not reached); echo "status $?"
