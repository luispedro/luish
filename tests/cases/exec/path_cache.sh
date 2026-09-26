# As in dash, a cached command is used without checking it; if its file is
# gone, the PATH directories after it are tried (dash's shellexec).
mkdir -p one two three
PATH="one:two:three:$PATH"
echo 'echo two' > two/mycmd
chmod +x two/mycmd
mycmd
echo 'echo one' > one/mycmd
chmod +x one/mycmd
mycmd
hash -r
mycmd
rm one/mycmd
echo 'echo three' > three/mycmd
chmod +x three/mycmd
mycmd; echo "status=$?"
rm two/mycmd three/mycmd
mycmd; echo "status=$?"
# Entries from relative directories are dropped by cd.
echo 'echo one' > one/other
chmod +x one/other
other
mkdir sub; cd sub
other; echo "status=$?"
cd ..
other
