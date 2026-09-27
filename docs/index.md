# luish

luish is a shell for Linux, written in Rust.

It can replace zsh or bash as your interactive shell, and dash or bash as the shell that runs scripts.

It is fast, and full featured: tab completion, history, line editing, globbing, and a modern plugin architecture.


## Highlights

**A modern plugin architecture.** Plugins can include shell files, and
extensions written in [Rhai](https://rhai.rs), a small embedded language, for
hooks, prompt variables and Tab completion. They can be automatically fetched
from github or other git repositories and pinned to specific commits.

**As fast as dash, with zsh's features.** Scripts run as fast as under dash
(see [](performance.md)) while interactive use is intended to be as
full-featured as zsh.

**Instant startup through caching.** luish caches the *effect* of your startup
files. The result is that a new shell starts instantly (&lt; 20ms) even if you
are using `conda`, `nvm` and the like (which can take several seconds in a
normal shell).

**Modern configuration.** You can use [TOML](https://toml.io) file
(`~/.config/luish/config.toml`) to set options, aliases, and enable plugins.
Options are grouped in meaningful categories, for example:


```toml
[options.history]
file = "~/.histfile"
share = true

[alias]
ll = "ls -l"

[plugins.enabled]
std.completion = "*"         # completion for common commands, and git
```

Plugins can also set options. You can have a personal plugin on github that
sets your favorite options, aliase, plugins, and functions. Then just enable it
for every machine you use and keep your configuration in sync.

## Getting started

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
usage
globbing
builtins
plugins
improvements
performance
compatibility
whatsnew
```
