# `__luish_internal style` is `style` in any shell: it shows and changes the
# styles of the line editor, by dotted names that fall back to their
# parents, in colour schemes and over them.
s() { __luish_internal style "$@"; }
# An error's status and message.
e() { s "$@" 2>err; echo "status $?: $(sed -n '1s/.*__luish_internal style: //p' err)"; }
echo '--- a name, with where it comes from'
s keyword
s command.alias
echo '--- setting one, over the scheme'
s command.alias bold
s command.alias
s comment italic 'bright-black bg:#102030' no-dim
s comment
s git.branch 'sgr:1;38;5;208'
s git.branch.dirty
echo '--- plain stops the fallback'
s command.function plain
s command.function
echo '--- errors'
e command.unknwn
e comand red
e keyword bolt
e keyword red blue
e colorscheme red
e -x
echo '--- schemes'
s -s blue keyword 'bold blue'
s -s blue command blue
s -s blue 'command.unknown' bold red
s -s green -i blue
s -s green keyword bold green
s -s green
s -s green keyword
s -s green string
e -s blue -i green
s -c green
s -c
s command.alias
s command.unknown
e -c nosuch
echo '--- a pair, by the background'
s -c green blue
s -c
COLORFGBG='0;15' s -c
LUISH_BACKGROUND=dark COLORFGBG='0;15' s -c
s -c green blue default-dark
s -c
echo '--- the built-in pair'
s -c default-dark default-light
COLORFGBG='0;15' s string
COLORFGBG='0;15' s keyword
echo '--- -p, and the saved state'
s -r command.alias comment
s -p
__luish_internal savestate | grep -c '^__luish_internal style'
__luish_internal savestate > state
$SH -c '. ./state; __luish_internal style -p' | diff - <(s -p) && echo same
echo '--- removing, and deleting a scheme'
s -s green -r keyword
s -s green
s -s green --delete
e -s green
s -s default-dark keyword italic
s keyword
s -s default-dark --delete
s keyword
echo '--- --clear: nothing'
s --clear
s keyword
s -c
s -p
echo '--- the list'
s -c default-dark
s | head -3
s | grep -c .
