# A remembered command that has moved or gone falls through to the rest of
# PATH, both in the shell and in a forked process (a pipeline, a subshell).
mkdir a b
printf '#!/bin/sh\necho tool\n' > a/tool
chmod +x a/tool
PATH=$PWD/a:$PWD/b:$PATH
tool
mv a/tool b/tool
tool; echo moved $?
tool | cat; (tool)
rm b/tool
tool; echo gone $?
tool | cat; (tool); echo $?
printf '#!/bin/sh\necho tool\n' > b/tool
chmod +x b/tool
tool; echo back $?
chmod -x b/tool
tool; echo noexec $?
