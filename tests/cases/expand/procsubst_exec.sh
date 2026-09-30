# reference: zsh
# `exec` keeps the descriptor after the command.
exec 3< <(echo from fd 3; echo second)
read a <&3
read b <&3
echo "$a / $b"
exec 3<&-
