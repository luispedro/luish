# Where luish's modifiers differ from zsh's: `..` at the root is the root
# for :a and :A (zsh gives `//a` for /../a); an unknown modifier is a bad
# substitution when expanded, with status 2 (zsh: "unrecognized modifier",
# status 1, when parsed); ${#x:h} is a bad substitution (zsh: the length of
# ${x:h}).
x=/../a; echo "${x:a} ${x:A} ${x:a:h}"
x=/a/../../b/; echo "${x:a}"
$SH -c 'x=a/b; echo "${x:h}"; echo "${x:hx}"; echo notreached' 2>/dev/null; echo "status $?"
$SH -c 'x=a/b; echo "${x:h:}"' 2>/dev/null; echo "status $?"
$SH -c 'x=a/b; echo "${#x:h}"' 2>/dev/null; echo "status $?"
$SH -c 'x=a/b; echo "${x:h-default}"' 2>/dev/null; echo "status $?"
