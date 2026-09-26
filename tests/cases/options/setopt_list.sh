# setopt and unsetopt without arguments list the options that are on and
# off, in alphabetical order, including luish's own (zsh has many more).
set -f -u
setopt prompt_percent
setopt
echo ---
unsetopt
echo ---
unsetopt Prompt_Percent
setopt
setopt promptpercent bogus 2>&1; echo "status $?"
# set -o and $- don't show luish's own options, as in dash.
set -o | grep -c promptpercent
(set -o promptpercent) 2>&1; echo "status $?"
