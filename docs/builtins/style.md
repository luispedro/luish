# `style`

```text
style [-p]
style name [value...]
style -r name...
style --clear
style -c [scheme | dark light [default]]
style --detect
style -s scheme [name [value...] | -r name... | -i [parent] | --delete]
style -d name value... | -d -r name...
```

Show or change the line editor's styles and colour scheme.

A style is a name, such as `keyword` or `command.unknown`, and a value:
words that give a colour, a background colour and attributes. The names
are dotted, and a style that is not set takes what it doesn't say from
its parent (`command.alias` from `command`). See the user documentation
on syntax highlighting for the names and values, and on colour schemes.

With no arguments, `style` lists every name with the style it has in the
scheme in use. With a name, it shows that style, and where each part of
it comes from. With a name and a value, it sets the style, over what the
colour scheme gives; the value's words may be one argument or several:

```sh
style command.unknown bold red
style comment 'italic bright-black'
```

`-r` removes the styles of the names given, which then come from the
colour scheme again. `--clear` removes all of them and chooses no scheme,
so that nothing is styled, as a starting point. `-p` prints commands that
recreate the schemes defined, the scheme chosen and the styles set.

`-c` with a scheme chooses it. With two, it chooses the first if the
terminal's background is dark and the second if it is light, and with a
third, that one if the background is not known (otherwise the first). The
background is known from `$LUISH_BACKGROUND` (`dark` or `light`), which
the shell sets before its first prompt from `$COLORFGBG` (which some
terminals set) or else by asking the terminal for its background colour.
`--detect` asks the terminal again, after its colours have changed, and
fails if it doesn't tell. `-c` alone lists the schemes, marking the one in
use.

`-s` defines or changes a scheme, as a `[colorscheme.NAME]` table in
`config.toml` does: with a name and a value, it sets the style in the
scheme (defining it if it isn't), with a name alone it shows it, and with
nothing else it lists the scheme. `-r` removes styles from the scheme,
`-i` sets the scheme it inherits from (none if empty), and `--delete`
removes the scheme (a built-in one goes back to how it was).

`-d` sets or removes a default, as the `[style]` table of a plugin's
`plugin.toml` does: under the colour scheme.

The status is 1 for an unknown name (with a suggestion if one is close), a
bad value or a scheme that doesn't exist, and 2 for a bad option.
