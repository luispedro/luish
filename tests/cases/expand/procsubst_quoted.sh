# Only at the start of a word, and not quoted.
echo "<(true)" '<(true)' \<
echo a\<b > /dev/null
echo x
