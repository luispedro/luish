# `bindkey`

```text
bindkey [-L] [key [widget]]
bindkey -r key...
bindkey -e | -v
```

Show or change the line editor's key bindings, as in zsh.

With no arguments, `bindkey` lists the bindings, one per line as
`"KEY" widget`; with `-L`, as `bindkey` commands. With a key, it shows what
the key is bound to, and with a key and a widget, it binds the key to the
widget. `bindkey -r` removes luish's binding of each key, which then does
what the line editor does by default. `-e` selects emacs mode and `-v` vi
mode (as `set -o emacs` and `set -o vi` do). `-M emacs` or `-M main` may be
given; there are no other keymaps.

Keys can be written by name: `Up`, `Down`, `Left`, `Right`, `Home`, `End`,
`Insert` (or `Ins`), `Delete` (`Del`), `PageUp` (`PgUp`), `PageDown`
(`PgDn`), `F1` to `F12`, `Tab`, `Enter` (`Return`), `Backspace`, `Esc`
and `Space`, or as the character they type, after any of the modifiers
`Ctrl-`, `Alt-` and `Shift-` (also `C-`, `M-` or `Meta-`, and `S-`, and
with `+` for `-`). Names and modifiers can be in any case, but characters
can't: `Alt-b` and `Alt-B` are different keys (`Alt-B` is `Alt-Shift-b`).
A sequence of keys is written with spaces between them:

```sh
bindkey Up up-line-or-history
bindkey Ctrl-Right forward-word
bindkey 'Ctrl-X Ctrl-E' undo
```

Keys can also be written as in zsh: `^X` is Ctrl-X (and `^?` is
Backspace), `^[` or `\e` is Esc, which comes before a key typed with Alt
(`^[.` is Alt-.), and `\C-x` and `\M-x` are Ctrl-X and Alt-X. Other keys
are written as the escape sequences that xterm sends: `^[[A` is Up (`^[OA`
is the same key), `^[[1;5C` is Ctrl-Right, `^[[3~` is Delete. A key may be
a sequence of keys, as `^X^E`. An argument is read this way unless it is
made of names (or characters with modifiers), so `^X Up` is Ctrl-X, space,
U and p; to bind the characters of a name, write the first one with a
backslash (`\Up`) or in octal, as `bindkey` shows them (`\125p`).
Listings always show the escape sequences.

The bindings apply in emacs mode. In vi mode the keys are the line
editor's own. The widgets are zsh's, by the same names:

`accept-line`, `accept-line-and-down-history` (run the line, and start the
next one with the history entry after it), `backward-char`,
`backward-delete-char`, `backward-kill-line`, `backward-kill-word`,
`backward-word`, `beginning-of-buffer-or-history`, `beginning-of-history`,
`beginning-of-line`, `capitalize-word`, `clear-screen`, `delete-char`,
`down-case-word`, `down-history`, `down-line-or-history`,
`end-of-buffer-or-history`, `end-of-history`, `end-of-line`,
`expand-or-complete`, `forward-char`, `forward-word`,
`history-beginning-search-backward` and `-forward` (the previous or next
history entry that starts with the text before the cursor),
`history-incremental-search-backward` and `-forward`, `insert-last-word`
(the last word of the previous command; again, that of the one before),
`kill-buffer`, `kill-line`, `kill-whole-line`, `kill-word`,
`quoted-insert`, `redisplay`, `send-break`, `transpose-chars`,
`transpose-words`, `undefined-key` (do nothing), `undo`, `up-case-word`,
`up-history`, `up-line-or-history`, `yank` and `yank-pop`.

The word widgets take as words the letters and digits and the characters
in `$WORDCHARS` (zsh's default, `*?_-.[]~=/&;!#$%^(){}<>`, when it is
unset), so with `WORDCHARS` not holding `/`, Ctrl-W removes one component
of a path.

The default bindings are zsh's for emacs mode, except that Up and Down
(`^[[A` and `^[[B`) are bound to `history-beginning-search-backward` and
`-forward`: with an empty line they go through the history as usual.

`bindkey` is a built-in only in interactive shells (and their subshells).
Anywhere, `__luish_internal bindkey` does the same. The exit status is 1 if
a key sequence or a widget is not valid, or a key can't be typed (such as
`Ctrl-Tab`, which terminals send as Tab).

Key bindings can also be set in `config.toml`, in its `bindkey` table.
