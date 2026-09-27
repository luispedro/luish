# `__luish_internal bindkey` is `bindkey` in any shell: it shows and
# changes the line editor's key bindings, as zsh's.
b() { __luish_internal bindkey "$@"; }
b '^W'
b '\C-w'
b '^[[A'
b '\eOA'
b -L '^[.'
b '^X^E'
b '^W' kill-whole-line
b '^W'
b '^X^E' undo
b -M emacs -L '^X^E'
b -r '^[[A'
b '^[[A'
b | grep -c .
b '^W' nosuch-widget 2>/dev/null; echo "unknown widget: $?"
b -M viins 2>/dev/null; echo "unknown keymap: $?"
b '^[[' 2>/dev/null; echo "bad sequence: $?"
b -q 2>/dev/null; echo "bad option: $?"
# Changes are saved as commands.
__luish_internal savestate | grep "^__luish_internal bindkey"
__luish_internal savestate > state
$SH -c '. ./state; __luish_internal bindkey "^W"; __luish_internal bindkey "^X^E"'
# Binding a key to its default is no longer a change.
b '^W' backward-kill-word
__luish_internal savestate | grep -c "^__luish_internal bindkey"
# -e and -v select the editing mode.
b -v; case $- in *V*) echo vi;; esac
b -e; case $- in *E*) echo emacs;; esac
