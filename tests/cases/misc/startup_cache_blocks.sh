# The startup cache's entries: one per file of rc.d and login.d, keyed on
# PATH and HOME, and one per `__luish_cache` block, keyed on the variables
# of its env=(...) and the files of its files=(...). A file with blocks runs
# every time, apart from its blocks.
PATH=/usr/bin:/bin
mkdir -p .config/luish/login.d bin other
cd .config/luish/login.d
cat > 10-path.lsh <<'X'
echo running 10-path
PATH=$HOME/bin:$PATH
X
cat > 20-tools.lsh <<'X'
echo uncached 20-tools
__luish_cache env=(TOOL) files=("$HOME/tool.conf" ~/missing) {
    # A comment.
    echo "building tools for $TOOL"
    TOOLS="$TOOL $(cat "$HOME/tool.conf" 2>/dev/null)"
    false
}
echo "block status $?"
__luish_cache {
    echo outer block
    __luish_cache { echo inner block; }
}
X
echo 'echo running 30-after; AFTER="after $TOOLS"' > 30-after.lsh
cat > _uncached.lsh <<'X'
__luish_cache env=(TOOL) { echo uncached block; U=$TOOL; }
X
cd
echo conf1 > tool.conf
show() {
    $SH -l -c 'echo "PATH=$PATH TOOLS=$TOOLS AFTER=$AFTER U=$U"' | sed "s|$HOME|~|g"
}
echo '--- built, then used'
TOOL=a show
TOOL=a show
echo '--- another TOOL: only the blocks that use it run again'
TOOL=b show
echo '--- both are kept'
TOOL=a show
TOOL=b show
echo '--- another PATH: the files run again, with the PATH they are given'
PATH=/bin TOOL=a show
PATH=/bin TOOL=a show
TOOL=a show
echo '--- a file in files=(...) changed or created'
echo conf2 > tool.conf
TOOL=a show
touch missing
TOOL=a show
TOOL=a show
# The files after the one changed run again (its uncached part may do
# something else), but not the block.
echo '--- a comment changed: the block doesn'"'"'t run again'
sed -i 's/A comment/Another comment/' .config/luish/login.d/20-tools.lsh
TOOL=a show
echo '--- the block changed: it runs again, and the files after it'
sed -i 's/echo "building tools/echo "now building tools/' .config/luish/login.d/20-tools.lsh
TOOL=a show
TOOL=a show
echo '--- check-cache rebuilds a block whose result changed'
# Its fingerprint is the same.
touch -r tool.conf reference
echo conf3 > tool.conf
touch -r reference tool.conf
TOOL=a show
TOOL=a $SH -c '__luish_internal check-cache -q login; echo "status $?"' |
    sed -e "s|$HOME|~|" -e 's|\(cache/luish/[a-z]*\)-.*|\1-HOST|' -e 's/generated .*/generated/'
TOOL=a show
echo '--- outside the startup files, a block just runs'
mkdir -p .config/luish
echo '__luish_cache env=(TOOL) { echo luishrc block; }' > .config/luish/luishrc
$SH -i -c : 2>/dev/null
$SH -i -c : 2>/dev/null
