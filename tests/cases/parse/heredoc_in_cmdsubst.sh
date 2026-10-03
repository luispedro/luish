# A here-document in $(...) with no body before the `)` is empty, as in
# dash: the lines after it are commands.
echo "[$(cat <<E)]"
echo not the body
E() { echo E is a command; }
E
