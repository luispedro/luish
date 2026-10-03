# `<<<` without a word is a syntax error (in dash, `<<<` always is).
$SH -c 'cat <<<' 2>/dev/null; echo "status $?"
$SH -c 'cat <<< ;' 2>/dev/null; echo "status $?"
