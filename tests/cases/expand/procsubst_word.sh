# reference: zsh
# Process substitution inside a word, as in `--input=<(cmd)`. The path differs
# between shells (/dev/fd/N, /proc/self/fd/N), so only its shape is shown.
show() { printf '%s\n' "$@" | sed 's,/dev/fd/[0-9]*,FD,; s,/proc/self/fd/[0-9]*,FD,'; }
show --input=<(true)
show a<(true) x<(true)y >(true)z
show 2<(true)
x=<(true)
show "$x"
# Read the file through the option.
opt() { cat "${1#--input=}"; }
opt --input=<(echo through the option)
# A redirection still needs a blank or a word before `<`.
echo ok < /dev/null
[[ a<b ]] && echo lt
# Quoted or escaped, it is text.
show "a<(true)" 'a<(true)' a\<\(true
