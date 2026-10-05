# A bad glob qualifier's error names the field, as the qualifier may come
# from an expansion (here a zsh-style prompt ending in `%(...)`).
"$SH" -c 'setopt bareglobqual; y="%#%([root].#.x)"; echo $y' 2>&1 | sed -n 's/^.*: [0-9]*: //p'
"$SH" -c 'setopt bareglobqual
echo *(Z)' 2>&1 | sed -n 's/^.*: [0-9]*: //p'
