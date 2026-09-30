# sh::which finds a program as running it would (never a function or
# built-in), and sh::commands lists the executables in PATH, as the
# editor's command completion does.
mkdir a b sub
printf '#!/bin/sh\necho a\n' > a/tool; chmod +x a/tool
printf '#!/bin/sh\necho b\n' > b/tool; chmod +x b/tool
printf 'not executable\n' > a/plain
printf '#!/bin/sh\n' > b/plain; chmod +x b/plain
mkdir a/dir-tool
printf '#!/bin/sh\n' > a/tool2; chmod +x a/tool2
printf '#!/bin/sh\n' > b/tool2; chmod +x b/tool2
printf '#!/bin/sh\n' > here; chmod +x here
printf '#!/bin/sh\n' > sub/noexec
fn_tool() { :; }
cat > which.rhai <<'P'
fn show(p) {
    if p == () {
        return "()";
    }
    p.replace(sh::getvar("HOME"), "HOME");
    p
}
for n in ["tool", "plain", "dir-tool", "fn_tool", "cd", "here", "./here", "sub/noexec", "missing", ""] {
    print(`${n}: ${show(sh::which(n))}`);
}
print(`commands t: ${sh::commands("t")}`);
print(`commands: ${sh::commands("")}`);
P
cat > gone.rhai <<'P'
let p = sh::which("tool");
p.replace(sh::getvar("HOME"), "HOME");
print(`gone: ${p}`);
P
PATH=$PWD/a:$PWD/b:
__luish_internal plugin load ./which.rhai
# A remembered command that was removed: the next one in PATH.
tool
command -p rm a/tool
__luish_internal plugin load ./gone.rhai
tool
