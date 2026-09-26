# Only interactive shells keep a history. (Debian's dash has no fc at all.)
fc -l
echo "status $?"
fc -s
echo "status $?"
