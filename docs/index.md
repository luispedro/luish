# luish

::::{div} luish-hero

{.luish-tagline}
🐚 A fast, modern shell for Linux, written in Rust.

luish can replace zsh or bash as your interactive shell, and dash or bash as the shell that runs scripts: as fast
as dash, with tab completion, history, line editing, globbing, and a modern plugin architecture.

Install it in one line (Linux on x86_64 or aarch64):

```sh
curl -fsSL https://raw.githubusercontent.com/luispedro/luish/main/install.sh | sh
```

:::{div} luish-buttons

```{button-ref} getting-started
:ref-type: doc
:color: primary
:shadow:

🚀 Get started
```

```{button-ref} installation
:ref-type: doc
:color: primary
:outline:

📦 Other ways to install
```

```{button-ref} whatsnew
:ref-type: doc
:color: primary
:outline:

✨ What's new
```

:::

::::

::::{grid} 1 3 3 3
:gutter: 2

:::{grid-item-card} up to 5×
:class-card: luish-stat
:shadow: none
:link: performance
:link-type: doc

faster than bash and zsh on scripts, and as fast as dash
:::

:::{grid-item-card} ~8 ms
:class-card: luish-stat
:shadow: none
:link: usage
:link-type: doc

to start an interactive shell, even with nvm or conda set up
:::

:::{grid-item-card} 270+
:class-card: luish-stat
:shadow: none
:link: plugins
:link-type: doc

commands with Tab completion, with the standard plugins and luish-extra
:::

::::

## Highlights

::::{grid} 1 1 2 2
:gutter: 3

:::{grid-item-card} 🧩 A modern plugin architecture
:class-card: luish-feature
:link: plugins
:link-type: doc

Plugins package configuration and shell files, and can include an extension written in Rhai, a small embedded
language, for hooks, prompt variables, Tab completion and commands. They are fetched from GitHub or any git
repository, and pinned to specific commits.
:::

:::{grid-item-card} 🌍 Remote shells that feel local
:class-card: luish-feature
:link: ssh
:link-type: doc

`luish --ssh HOST` runs your commands on HOST, but edits the command line on your own machine: typing, history and
the completion menu don't wait for the network, however slow the connection. luish needn't be installed on HOST, as it
copies itself there.
:::

:::{grid-item-card} ⚡ As fast as dash, with zsh's features
:class-card: luish-feature
:link: performance
:link-type: doc

Scripts run as fast as under dash, while interactive use is meant to be as full-featured as zsh's. What luish adds
costs scripts nothing unless they use it.
:::

:::{grid-item-card} 🧰 bash's and zsh's scripting extensions
:class-card: luish-feature
:link: usage
:link-type: doc

Arrays and associative arrays, `[[ ... ]]`, `${x/pattern/replacement}`, `typeset`, zsh's parameter flags and
`pipefail` work in scripts and interactively, without slowing down scripts that don't use them.
:::

:::{grid-item-card} 🏎️ Instant startup through caching
:class-card: luish-feature
:link: usage
:link-type: doc

luish caches the *effect* of your startup files, so a new shell starts in milliseconds even with `conda`, `nvm` and
the like, which can take seconds in other shells.
:::

:::{grid-item-card} ⚙️ Modern configuration
:class-card: luish-feature
:link: getting-started
:link-type: doc

Set options, aliases and plugins in a TOML file, with options grouped in meaningful categories.
A personal plugin on GitHub can carry your configuration to every machine you use.
:::

::::

## ⚙️ Configuration at a glance

`~/.config/luish/config.toml` sets options, aliases, and the plugins to enable:

```toml
[options.history]
file = "~/.histfile"
share = true

[alias]
ll = "ls -l"

[plugins.enabled]
std.completion = "*"         # completion for common commands, and git
```

[luish-extra](plugins.md#more-completion-luish-extra) adds completion for about 270 more commands, from
bioinformatics, science, desktop and Python tools.

Plugins can also set options. You can have a personal plugin on GitHub that sets your favourite options, aliases,
plugins, and functions, then enable it on every machine you use to keep your configuration in sync (see
[](personal-plugin.md)).

## 🚀 Getting started

Install luish (see [](installation.md)), and start it:

```sh
luish                             # an interactive shell
luish script.sh                   # run a script, as sh script.sh does
```

[](getting-started.md) then sets up a first configuration, with the standard plugins, a few aliases and good
options.

```{toctree}
:hidden:
:maxdepth: 2

installation
getting-started
personal-plugin
usage
ssh
colour-schemes
globbing
builtins
plugins
extensions
improvements
comparison/index
performance
compatibility
whatsnew
```
