# Tab expands a word with a glob, `$` or a substitution in it, as zsh's
# expand-or-complete does, and completes it otherwise. Several words are
# followed by a space. `__luish_internal complete LINE` shows what Tab does.
mkdir sub
touch a.md b.md 'c d.md' x.txt sub/y.md
X='one two'
EMPTY=
c() {
    echo "--- $1"
    __luish_internal complete "$1"
    echo "status $?"
}
c 'ls *.md'
c 'ls sub/*'
c 'ls [ab].md'
c 'ls a?md'
c 'echo $X'
c 'echo "$X"'
c 'echo x$(echo hi)y'
c 'echo `echo a b`'
c 'ls *.md > x*'
# No expansion: the matches of the word as typed.
c 'ls *.zz'
c 'ls "*.md'
c 'ls \*.md'
c 'ls "*".md'
c 'echo $EMP'
c 'echo $EMPTY'
c 'ls ~/sub'
set -f
c 'ls *.md'
set +f
setopt glob.bare_qualifiers
c 'ls *.md(.)'
