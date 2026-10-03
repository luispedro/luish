# Making a colour scheme

A colour scheme is a named set of styles. It colours the command line as it is typed, the completion menu, the
autosuggestions and any prompt that uses named styles. This page shows how to make one: try it out in a running
shell, save it in `config.toml`, add a light version, and share it as a plugin. The names and values are listed in
[Syntax highlighting](usage.md#syntax-highlighting), and the rules for how a style is worked out are in
[Colour schemes](usage.md#colour-schemes).

## Try it out in the shell

`style -s` changes a scheme in the running shell, and the next prompt uses the change. So it is easiest to work in a
shell, with a command line that has a bit of everything to look at:

```sh
style -s ocean -i default-dark     # a new scheme, which takes what it doesn't set from default-dark
style -c ocean                     # use it
style -s ocean keyword 'bold #5f87ff'
style -s ocean command '#87d7af'
```

A line like this one shows most of the styles (type it rather than run it):

```sh
if [ -n "$HOME" ]; then nosuch 'one' "two $USER $(date)" ~/*.txt {a,b} 2>&1 | grep -v x && echo $UNSET; fi # done
```

Some commands that help along the way:

- `style -s ocean` lists what the scheme sets, and the scheme it inherits from.
- `style keyword` shows the style a name has, and where each part comes from (the scheme, a scheme it inherits from,
  your own settings or a plugin's defaults).
- `style` lists every name with the style it has now.
- `style -s ocean -r keyword` removes a name from the scheme again.
- `style -c` lists the schemes, marking the one in use.

## Choose what to set

The names are dotted, and a name that isn't set takes what it doesn't say from its parent: `command.alias` from
`command`, `string.double` from `string`. So set the general names first, which gives a usable scheme, then the
specific ones where you want them to stand out. These are enough for most of the line:

| Name | Sets |
|---|---|
| `keyword` | `if`, `then`, `for`, `{`, `[[` |
| `command` | all kinds of command names; then `command.unknown`, `command.function`, `command.alias`... |
| `arg` | arguments; then `arg.option` for `-x` |
| `string` | quoted text; then `string.single`, `string.double`, `string.escape`, `string.heredoc` |
| `var` | `$NAME` and `${...}`; then `var.unset`, `var.exported`, `var.special`... |
| `subst` | `$(...)`, backquotes, `<(...)`, `$((...))` |
| `expand` | `~`, `{a,b}`, `*` and `?` |
| `op`, `redir` | operators (`;`, `&&`, `\|`), and redirections (`>`, `2>&1`) |
| `assign`, `comment` | the `NAME=` of an assignment, and comments |

Some names aren't parts of the line, and need setting in any scheme that doesn't inherit them:

| Name | For | In `default-dark` |
|---|---|---|
| `error` | added to text with a syntax error | `red underline` |
| `path`, `path.prefix` | added to words that name files, with `setopt highlight.paths` | `underline` |
| `menu.selected` | the selection in the completion menu | `reverse` |
| `menu.description` | descriptions in the completion menu | `bright-black` |
| `suggestion` | autosuggestions | `bright-black` |
| `plugin.name`, `plugin.ok`, `plugin.update`, `plugin.warn`, `plugin.error`, `plugin.dim` | the output of `plugin` (names, what worked, what changed or can, warnings, errors, detail), on a terminal | `bold`, `green`, `yellow`, `yellow`, `bold red`, `bright-black` |

`error`, `path` and `path.prefix` are added on top of the text's own style. An attribute such as `underline` keeps
the text's colour; a colour replaces it.

## Start from a scheme, or from nothing

A scheme starts empty: it has only what it sets and what it inherits. Without `inherits`, what it doesn't set has the
terminal's default colours, including the completion menu's selection (which is then invisible: set
`menu.selected`).

Inheriting from `default-dark` is quicker, but its specific names win over your general ones: a name takes its
value from the first scheme that sets it, and only then falls back to its parent. With the scheme above, aliases are
still green:

```console
$ style command.alias
command.alias italic green
  command.alias: italic green (scheme default-dark)
  command: #87d7af (scheme ocean)
```

`command.alias` is `italic green` in `default-dark`, so it doesn't fall back to `ocean`'s `command`. Set it in your
scheme too. These are the names `default-dark` sets:

```toml
keyword = "bold blue"
command = "green"
"command.function" = "bold green"
"command.alias" = "italic green"
"command.unknown" = "bold red"
string = "yellow"
var = "cyan"
"var.exported" = "bold cyan"
"var.unset" = "dim cyan"
subst = "magenta"
expand = "blue"
op = "bold"
redir = "bold"
comment = "bright-black"
assign = "blue"
error = "red underline"
path = "underline"
"menu.selected" = "reverse"
"menu.description" = "bright-black"
suggestion = "bright-black"
```

## Pick the colours

The value of a style is words separated by spaces: a colour for the text, `bg:` and a colour for the background, and
attributes (`bold`, `dim`, `italic`, `underline`, `reverse`, `strike`, and `no-bold` and so on to turn off what the
parent has).

There are three kinds of colour, and the choice matters for who can use the scheme:

- **The 16 named colours** (`red`, `bright-blue`, ...) are those of the terminal's own palette, so they follow the
  terminal's theme: a scheme that only uses them often works on both dark and light backgrounds (`default-dark`
  differs from `default-light` only in `string`, as yellow is hard to read on white).
- **The 256 colours** (a number from 0 to 255) are the same in most terminals, apart from the first 16.
- **`#rrggbb`** is an exact colour, in terminals that support 24-bit colour (most do, but not the Linux console).
  An exact colour doesn't adapt to the background, so a scheme of them needs a light version (see below).

`dim` is a good way to mark something as less important (as `default-dark` does for `var.unset`), as it works with
any colour.

## Save it in `config.toml`

`style -p` prints commands that recreate the schemes you defined and the one you chose; schemes in the running shell
are lost when it exits. To keep one, write it in a `[colorscheme.NAME]` table in `~/.config/luish/config.toml`, and
choose it in the `[style]` table:

```toml
[colorscheme.ocean]
inherits = "default-dark"
keyword = "bold #5f87ff"
command = "#87d7af"
"command.alias" = "italic #87d7af"
"command.function" = "bold #87d7af"

[style]
colorscheme = "ocean"
```

Quote the names with a dot: in TOML, `command.alias = "..."` (without the quotes) would mean a key `alias` in a
table `command`.

The other keys of `[style]` are your own settings, over any scheme (as `style NAME VALUE` sets them). Keep the scheme
in its table, and use `[style]` for what you'd want whichever scheme is in use.

## A light version

A scheme for a light background can inherit the dark one and change only the colours that are hard to read:

```toml
[colorscheme.ocean-light]
inherits = "ocean"
keyword = "bold #005fd7"
command = "#00875f"
"command.alias" = "italic #00875f"
"command.function" = "bold #00875f"

[style]
colorscheme = { dark = "ocean", light = "ocean-light" }
```

The shell then chooses by the terminal's background, which it finds out before the first prompt (see [Colour
schemes](usage.md#colour-schemes)). To check the other version without changing the terminal, set
`LUISH_BACKGROUND`, and the next prompt uses it:

```sh
LUISH_BACKGROUND=light
```

`style -c` shows which is in use, and why. Add `default = "..."` to the pair for when the background isn't known;
without it, the dark scheme is used.

## The terminal's colours

The styles colour only the command line. A scheme can also set the terminal's own colours, so that its background
and the colours that other programs use match it:

```toml
[colorscheme.ocean.terminal]
background = "#1c2331"
foreground = "#d8dee9"
cursor = "#d8dee9"
palette = ["#1c2331", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d8dee9"]

[colorscheme.ocean-light.terminal]
background = "#fafafa"
foreground = "#383a42"
```

`palette` lists colours 0 to 15 of the terminal's palette (or the first few), which `ls --color`, `git diff` and
other programs use; set all 16 for a scheme that should look the same in every terminal. `ocean-light` inherits the
palette and the cursor from `ocean`, and gives its own background and text colour.

The shell sets them while the scheme is in use and puts back the terminal's own colours when it isn't, or when the
shell exits (see [The terminal's colours](usage.md#the-terminals-colours)). A scheme that sets the background no
longer depends on the terminal's being dark or light, but a dark/light pair still follows the terminal's own, so
each looks right with the others: try them with `style -c ocean` and `style -c ocean-light`. Those who'd rather keep
their terminal's colours set `terminal-colors = false` in their `[style]` table.

## Prompts

A scheme can also give styles to names of your own, for prompts: names that don't start like one of luish's, such as
`prompt.dir` or `prompt.error`. A prompt uses them with `%[style:NAME]` (with `setopt prompt.percent`; see
[Prompts](usage.md#prompts)):

```toml
[colorscheme.ocean]
# ...
"prompt.dir" = "bold #5f87ff"
"prompt.error" = "#ff5f5f"
```

```sh
PS1='%[style:prompt.dir]%[dir]%[style_off] %([status]..%[style:prompt.error][%[status]]%[style_off] )%[prompt_char] '
```

A prompt can also use the line's own styles, such as `%[style:keyword]`, so that it changes with the scheme without
names of its own.

## Share it as a plugin

A theme is a plugin whose `plugin.toml` has the `[colorscheme.NAME]` tables, written as in `config.toml` (see
[Themes](plugins.md#themes)). A directory with only that file is enough:

```text
ocean-theme/
└── plugin.toml
```

```toml
description = "Ocean: blue and green, for dark and light terminals"

[colorscheme.ocean]
inherits = "default-dark"
keyword = "bold #5f87ff"
command = "#87d7af"

[colorscheme.ocean-light]
inherits = "ocean"
keyword = "bold #005fd7"
command = "#00875f"
```

Loading the plugin only makes the schemes available; users choose them in their own `[style]` table, or with
`style -c` (a plugin can't choose one). So say in the plugin's README which names to use:

```toml
# config.toml
[plugins.enabled]
ocean-theme = { gh = "someone/ocean-theme" }

[style]
colorscheme = { dark = "ocean", light = "ocean-light" }
```

A plugin's `[style]` table holds defaults for the names it uses itself (such as a prompt's `git.branch`), which work
with any scheme; a scheme, or the user, can override them.

When you define a pair, test both on both backgrounds, as above, and with `setopt highlight.paths` on, since `path`
is added to many words then.
