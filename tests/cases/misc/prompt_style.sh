# `%[style:NAME]` sets the style NAME (as `style` sets it), on top of the
# attributes and colours in effect, and `%[style_off]` goes back to those
# (to the terminal's defaults at the outermost level). Each writes the whole
# state (`ESC[0;...m`). Only the branch of `%(...)` that is taken counts.
# With $NO_COLOR, both do nothing. Bad names are errors.
$SH -c 'setopt promptpercent
__luish_internal style prompt.dir bold blue
__luish_internal style prompt.err red
__luish_internal style prompt.plain plain underline
PS4="[%[style:prompt.dir]d%[style:prompt.err]e%[style_off]d%[style_off]] "
set -x; : nested
PS4="[%B%F{green}%[style:prompt.err]e%[style_off]g%b%f%[style_off]] "
: around attributes
PS4="[%S%[style:prompt.plain]p%[style_off]%s|%[style:nothing]n%[style_off]|%[style:command.unknown]u%[style_off]] "
: plain, unset, a role
PS4="[%(?.%B.%[style:prompt.err])x%[style:prompt.dir]y%[style_off]%b%(?..%[style_off])] "
: conditions; (exit 1); : after
set +x
NO_COLOR=1
PS4="[%[style:prompt.dir]d%[style_off]] "
set -x; : no colour' sh 2>&1 | cat -v
echo '--- errors'
$SH -c 'setopt promptpercent
PS4="[%[style]|%[style:comand]|%[style:command.nope]|%[style:a..b]|%[stlye:x]] "
set -x; : one' sh 2>&1
