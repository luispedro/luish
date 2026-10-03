# A subscript read as in double quotes, where '$(' starts a command
# substitution that doesn't parse: as in dash, a bad substitution (in
# which the quotes are quotes), and the here-document is read as usual.
f() {
  cat <<E; echo ${a['$(']}
body
E
}
echo ok
f
echo never
