# Sphinx configuration for the luish user documentation.
# Pages are written in Markdown (MyST). Build locally with `pixi run docs`.

project = "luish"
author = "Luis Pedro Coelho"
copyright = "2026, Luis Pedro Coelho"

extensions = ["myst_parser", "sphinx_copybutton", "sphinx_design"]
source_suffix = {".md": "markdown"}
# The pages in builtins/ are included by builtins.md (and compiled into
# luish for `help`).
exclude_patterns = ["_build", "builtins"]

myst_heading_anchors = 3
myst_enable_extensions = ["deflist", "colon_fence", "attrs_block"]

html_theme = "furo"
html_title = "luish"
html_static_path = ["_static"]
html_css_files = ["custom.css"]
html_theme_options = {
    "light_css_variables": {
        "color-brand-primary": "#0f7b6c",
        "color-brand-content": "#0f7b6c",
    },
    "dark_css_variables": {
        "color-brand-primary": "#3ccfb4",
        "color-brand-content": "#3ccfb4",
    },
}

# In examples of a session (`$ ` before each command), the copy button copies only the commands.
copybutton_prompt_text = "$ "


# The extension examples are Rhai, which Pygments doesn't know. Rust's lexer is
# close, but fails on Rhai's backtick strings (with `${...}` interpolation).
def setup(app):
    from pygments.lexer import include, inherit
    from pygments.lexers.rust import RustLexer
    from pygments.token import String

    class RhaiLexer(RustLexer):
        name = "Rhai"
        aliases = ["rhai"]
        filenames = ["*.rhai"]
        tokens = {
            "base": [(r"`", String.Backtick, "template"), inherit],
            "template": [
                (r"`", String.Backtick, "#pop"),
                (r"\$\{", String.Interpol, "interp"),
                (r"[^`$]+|\$", String.Backtick),
            ],
            "interp": [
                (r"\}", String.Interpol, "#pop"),
                include("base"),
            ],
        }

    app.add_lexer("rhai", RhaiLexer)
