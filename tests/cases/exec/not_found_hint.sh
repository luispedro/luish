# A command that isn't found gets a hint after `not found` if luish knows it
# from another shell: bash's `shopt` suggests `setopt`, with the options
# translated when luish knows their names.
hint() { "$SH" -c "$1" 2>&1 | sed "s|^$SH: ||"; }
hint 'shopt -s globstar autocd; echo "status $?"'
hint 'shopt -u globstar'
hint 'shopt -s extglob'
hint 'shopt'
hint 'shopt -s globstar; :'
hint 'x=1 shopt -s globstar'
hint 'command shopt -s globstar'
hint 'shopt -s globstar &
wait'
hint '(shopt -s globstar)'
hint 'echo x | shopt -s globstar'
# No hint for other commands, nor for `shopt` given with a path.
hint 'nosuchcommand_xyz -s globstar'
hint './shopt -s globstar'
# Nor when it's only looked up.
hint 'command -v shopt; echo "status $?"'
