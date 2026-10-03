# `clipcopy`

```text
clipcopy [file]
```

Put text on the clipboard, through the terminal.

`clipcopy` reads `file` (or standard input, if there is no `file` or it is
`-`) and asks the terminal to put it on the system clipboard, with the
escape sequence for that (OSC 52), written to `/dev/tty`. It works over
ssh, since the terminal on your side does it, and needs no `xclip`,
`wl-copy` or `pbcopy`:

```sh
git log -1 --format=%H | clipcopy
clipcopy ~/.ssh/id_ed25519.pub
```

The text is copied as it is, with its final newline if it has one.

Most terminals take it: kitty, WezTerm, Ghostty, foot, Alacritty, iTerm2
(once allowed in its settings), Windows Terminal and xterm (with
`allowWindowOps`), but not GNOME Terminal and other VTE ones. Some limit
how much text they take. In tmux it needs `set -g set-clipboard on`.

`clipcopy` is a built-in only in interactive shells (and their
subshells), so that scripts find the same commands as they do in dash.
`__luish_internal clipcopy` is the same command in any shell. A function
named `clipcopy` (such as oh-my-zsh's) is used instead of the built-in.

The status is 0 if the text was sent to the terminal, which doesn't say
whether the terminal took it; 1 if the file can't be read or there is no
terminal; 2 for a usage error.
