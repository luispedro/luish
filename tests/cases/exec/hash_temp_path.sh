# A temporary PATH (`PATH=... cmd`) is used to find the command, and (as in
# dash) clears the remembered commands.
mkdir a
printf '#!/bin/sh\necho mine\n' > a/cat
chmod +x a/cat
echo x | cat
PATH=$PWD/a:$PATH cat
hash | sed 's,.*/,,'
cat </dev/null; echo st $?
f() { cat </dev/null; echo in f; }
PATH=$PWD/a:$PATH f
cat </dev/null; echo st $?
