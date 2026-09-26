# Sphinx configuration for the luish user documentation.
# Pages are written in Markdown (MyST). Build locally with `pixi run docs`.

project = "luish"
author = "Luis Pedro Coelho"
copyright = "2026, Luis Pedro Coelho"

extensions = ["myst_parser"]
source_suffix = {".md": "markdown"}
# The pages in builtins/ are included by builtins.md (and compiled into
# luish for `help`).
exclude_patterns = ["_build", "builtins"]

myst_heading_anchors = 3
myst_enable_extensions = ["deflist"]

html_theme = "furo"
html_title = "luish"
