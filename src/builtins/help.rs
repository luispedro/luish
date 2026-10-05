//! `help`: shows the documentation of built-ins. The text is the Markdown
//! in `docs/builtins/`, which the user documentation includes too, shown
//! as plain text, or in the colours of the scheme in use on a terminal.
//!
//! `help` is a built-in only in interactive shells (see `INTERACTIVE` in
//! `mod.rs`); `__luish_internal help` is the same command everywhere.

use crate::builtins::style;
use crate::interactive::highlight::{self, Colors, CommandKind, Facts, VarKind, classify};
use crate::shell::{ExecResult, Shell};
use crate::style::Role;

macro_rules! page {
    ($file:literal) => {
        include_str!(concat!("../../docs/builtins/", $file, ".md"))
    };
}

/// (name, Markdown text), sorted by name. Each text starts with a `#`
/// heading, then a code block with the synopsis, then a one-line summary.
const TOPICS: &[(&[u8], &str)] = &[
    (b".", page!("dot")),
    (b":", page!("colon")),
    (b"[", page!("test")),
    (b"__luish_internal", page!("luish_internal")),
    (b"alias", page!("alias")),
    (b"bg", page!("bg")),
    (b"bindkey", page!("bindkey")),
    (b"break", page!("break")),
    (b"builtin", page!("builtin")),
    (b"caller", page!("caller")),
    (b"cd", page!("cd")),
    (b"chdir", page!("cd")),
    (b"clipcopy", page!("clipcopy")),
    (b"command", page!("command")),
    (b"continue", page!("continue")),
    (b"declare", page!("typeset")),
    (b"dirs", page!("dirs")),
    (b"echo", page!("echo")),
    (b"eval", page!("eval")),
    (b"exec", page!("exec")),
    (b"exit", page!("exit")),
    (b"export", page!("export")),
    (b"false", page!("false")),
    (b"fc", page!("fc")),
    (b"fg", page!("fg")),
    (b"getopts", page!("getopts")),
    (b"hash", page!("hash")),
    (b"help", page!("help")),
    (b"jobs", page!("jobs")),
    (b"kill", page!("kill")),
    (b"let", page!("let")),
    (b"local", page!("local")),
    (b"plugin", page!("plugin")),
    (b"popd", page!("dirs")),
    (b"print", page!("print")),
    (b"printf", page!("printf")),
    (b"pushd", page!("dirs")),
    (b"pwd", page!("pwd")),
    (b"read", page!("read")),
    (b"readonly", page!("readonly")),
    (b"return", page!("return")),
    (b"set", page!("set")),
    (b"setopt", page!("setopt")),
    (b"shift", page!("shift")),
    (b"source", page!("dot")),
    (b"style", page!("style")),
    (b"test", page!("test")),
    (b"times", page!("times")),
    (b"trap", page!("trap")),
    (b"true", page!("true")),
    (b"type", page!("type")),
    (b"typeset", page!("typeset")),
    (b"ulimit", page!("ulimit")),
    (b"umask", page!("umask")),
    (b"unalias", page!("unalias")),
    (b"unset", page!("unset")),
    (b"unsetopt", page!("setopt")),
    (b"wait", page!("wait")),
    (b"where", page!("where")),
];

/// `help`.
pub fn help(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// Shows the help for each of `args`, or the list of topics without any.
/// `name` is the command, for error messages.
pub fn run(sh: &mut Shell, name: &[u8], args: &[Vec<u8>]) -> ExecResult {
    let paint = Paint::new(sh);
    if args.is_empty() {
        return Ok(sh.out_status(index(paint.as_ref()).as_bytes()));
    }
    let mut status = 0;
    for (i, a) in args.iter().enumerate() {
        match TOPICS.iter().find(|t| t.0 == a.as_slice()) {
            Some((_, text)) => {
                let sep = if i > 0 { "\n" } else { "" };
                sh.out(format!("{sep}{}", render(text, paint.as_ref())).as_bytes());
            }
            None => {
                sh.berr(name, format!("no help for {}", String::from_utf8_lossy(a)));
                status = 1;
            }
        }
    }
    Ok(status)
}

/// The roles whose styles `help` uses, from the colour scheme in use:
/// headings, code spans and the command names of synopses.
const ROLES: [Role; 3] = [Role::of("keyword"), Role::of("string"), Role::of("command.builtin")];

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// The escape sequences for the colours of what `help` shows (each empty
/// for a role without a style).
struct Paint {
    heading: String,
    code: String,
    command: String,
    /// The line editor's colours, for the examples.
    colors: Colors,
}

impl Paint {
    /// The colours, or none for plain text (see `stdout_sgr`).
    fn new(sh: &Shell) -> Option<Paint> {
        let [heading, code, command] = style::stdout_sgr(sh, ROLES)?;
        let scheme = style::scheme_in_use(sh);
        let r = sh.styles.resolver(scheme.as_deref());
        Some(Paint {
            heading,
            code,
            command,
            colors: Colors::new(|n| r.get(n)),
        })
    }

    /// How code spans are shown in text that is in `base`: in colour,
    /// unless `paint` is none or code has no style, then `plain`.
    fn code<'a>(paint: Option<&'a Paint>, base: &'a str, plain: Code<'a>) -> Code<'a> {
        match paint {
            Some(p) if !p.code.is_empty() => Code::Paint { on: &p.code, base },
            _ => plain,
        }
    }
}

/// How `inline` shows code spans.
#[derive(Clone, Copy)]
enum Code<'a> {
    /// With their backticks.
    Ticks,
    /// Without.
    Bare,
    /// Without, in `on`, then back to `base`.
    Paint { on: &'a str, base: &'a str },
}

/// Adds `text` to `out` in `on` (unless it is empty).
fn wrap(out: &mut String, on: &str, text: &str) {
    if on.is_empty() {
        out.push_str(text);
    } else {
        out.push_str(on);
        out.push_str(text);
        out.push_str(RESET);
    }
}

/// The list of topics, each with its summary.
fn index(paint: Option<&Paint>) -> String {
    let code = Paint::code(paint, "", Code::Ticks);
    let mut out = inline("Built-in commands (`help NAME` shows more about one):", code);
    out.push_str("\n\n");
    for (name, text) in TOPICS {
        let name = String::from_utf8_lossy(name);
        out.push_str("  ");
        wrap(&mut out, paint.map_or("", |p| &p.command), &name);
        out.push_str(&" ".repeat(18usize.saturating_sub(name.len())));
        out.push_str(&summary(text, code));
        out.push('\n');
    }
    out
}

/// The line after the synopsis.
fn summary(md: &str, code: Code) -> String {
    let mut in_code = false;
    let mut seen_code = false;
    for line in md.lines().skip(1) {
        if line.starts_with("```") {
            in_code = !in_code;
            seen_code = true;
        } else if seen_code && !in_code && !line.is_empty() {
            return inline(line, code);
        }
    }
    String::new()
}

/// Renders Markdown for a terminal, as plain text or with `paint`'s
/// colours. Only what the pages use is handled: the heading of the page
/// (left out), headings, code blocks (the first one, the synopsis, flush
/// left, the others indented, with `sh` ones highlighted as the line
/// editor does), definition lists (`: ` lines and their continuations
/// indented, terms in bold), `**`, links and backslash escapes. In plain
/// text, code spans keep their backticks, except in headings and the terms
/// of definition lists.
fn render(md: &str, paint: Option<&Paint>) -> String {
    let text_code = Paint::code(paint, "", Code::Ticks);
    let term_code = Paint::code(paint, BOLD, Code::Bare);
    let (bold, heading_sgr) = paint.map_or(("", ""), |p| (BOLD, p.heading.as_str()));
    let mut out = String::new();
    // The code block being read: its language and its lines.
    let mut block: Option<(&str, Vec<&str>)> = None;
    let mut synopsis = true;
    let mut definition = false;
    let mut lines = md.lines().skip(1).skip_while(|l| l.is_empty()).peekable();
    while let Some(line) = lines.next() {
        if let Some(lang) = line.strip_prefix("```") {
            match block.take() {
                Some((lang, body)) => code_block(&mut out, lang, &body, std::mem::take(&mut synopsis), paint),
                None => block = Some((lang, Vec::new())),
            }
            continue;
        }
        if let Some((_, body)) = &mut block {
            body.push(line);
            continue;
        }
        if let Some(def) = line.strip_prefix(": ") {
            definition = true;
            out.push_str("    ");
            out.push_str(&inline(def, text_code));
        } else if definition && line.starts_with("  ") {
            out.push_str("  ");
            out.push_str(&inline(line, text_code));
        } else {
            definition = false;
            let heading = line
                .trim_start_matches('#')
                .strip_prefix(' ')
                .filter(|_| line.starts_with('#'));
            if let Some(h) = heading {
                wrap(&mut out, heading_sgr, &inline(h, Code::Bare));
            } else if lines.peek().is_some_and(|l| l.starts_with(": ")) {
                wrap(&mut out, bold, &inline(line, term_code));
            } else {
                out.push_str(&inline(line, text_code));
            }
        }
        out.push('\n');
    }
    out
}

/// Adds a code block to `out`: the synopsis flush left, with its command
/// names coloured, others indented, with shell code (`sh`) highlighted.
fn code_block(out: &mut String, lang: &str, body: &[&str], synopsis: bool, paint: Option<&Paint>) {
    let indent = if synopsis { "" } else { "    " };
    let highlighted = paint.filter(|_| lang == "sh").map(|p| highlight(body, &p.colors));
    for (i, line) in body.iter().enumerate() {
        out.push_str(indent);
        match (paint, &highlighted) {
            (_, Some(h)) => out.push_str(&h[i]),
            // Continuation lines start with spaces.
            (Some(p), None) if synopsis && !line.starts_with(' ') => {
                let (name, rest) = line.split_at(line.find(' ').unwrap_or(line.len()));
                wrap(out, &p.command, name);
                out.push_str(rest);
            }
            _ => out.push_str(line),
        }
        out.push('\n');
    }
}

/// The lines of shell code `body`, highlighted as the line editor would,
/// whatever the shell's functions, aliases and variables are: a command
/// name is a built-in or else an external command.
fn highlight(body: &[&str], colors: &Colors) -> Vec<String> {
    let text = body.join("\n");
    let builtins: Vec<&[u8]> = crate::builtins::names().collect();
    let command = |name: &[u8]| match builtins.contains(&name) {
        true => CommandKind::Builtin,
        false => CommandKind::External,
    };
    let facts = Facts {
        command: &command,
        var: &|_| Some(VarKind::Plain),
        braces: true,
        glob: true,
        path: None,
        home: None,
    };
    let cells = classify(text.as_bytes(), None, &facts);
    let mut start = 0;
    (body.iter())
        .map(|line| {
            let range = start..start + line.len();
            start = range.end + 1;
            let s = highlight::render(&text.as_bytes()[range.clone()], &cells[range], colors);
            String::from_utf8_lossy(&s).into_owned()
        })
        .collect()
}

/// Renders the inline markup of a line: `**`, links and backslash escapes,
/// and code spans, shown as `code` says.
fn inline(line: &str, code: Code) -> String {
    let b = line.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut strong = false;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' => {
                let n = b[i..].iter().take_while(|&&c| c == b'`').count();
                let fence = &b[i..i + n];
                // The closing run of exactly `n` backticks.
                let close = (i + n..b.len())
                    .find(|&j| b[j..].starts_with(fence) && b.get(j + n) != Some(&b'`') && b[j - 1] != b'`');
                let Some(j) = close else {
                    out.extend_from_slice(fence);
                    i += n;
                    continue;
                };
                let mut span = &b[i + n..j];
                if span.len() > 2 && span[0] == b' ' && span[span.len() - 1] == b' ' {
                    span = &span[1..span.len() - 1];
                }
                match code {
                    Code::Ticks => out.extend_from_slice(&b[i..j + n]),
                    Code::Bare => out.extend_from_slice(span),
                    Code::Paint { on, base } => {
                        out.extend_from_slice(on.as_bytes());
                        out.extend_from_slice(span);
                        out.extend_from_slice(RESET.as_bytes());
                        out.extend_from_slice(base.as_bytes());
                    }
                }
                i = j + n;
            }
            b'*' if b.get(i + 1) == Some(&b'*') => {
                if let Code::Paint { .. } = code {
                    strong = !strong;
                    out.extend_from_slice(if strong { BOLD } else { "\x1b[22m" }.as_bytes());
                }
                i += 2;
            }
            b'\\' if b.get(i + 1).is_some_and(u8::is_ascii_punctuation) => {
                out.push(b[i + 1]);
                i += 2;
            }
            b'[' => {
                // `[text](url)` shows the text.
                let link = b[i..].iter().position(|&c| c == b']').and_then(|e| {
                    let e = i + e;
                    (b.get(e + 1) == Some(&b'(')).then_some(e)?;
                    let end = e + 2 + b[e + 2..].iter().position(|&c| c == b')')?;
                    Some((e, end))
                });
                match link {
                    Some((e, end)) => {
                        out.extend_from_slice(inline(&line[i + 1..e], code).as_bytes());
                        i = end + 1;
                    }
                    None => {
                        out.push(b'[');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_help() {
        for name in crate::builtins::names() {
            assert!(TOPICS.iter().any(|t| t.0 == name), "{}", String::from_utf8_lossy(name));
        }
    }

    #[test]
    fn topics_sorted() {
        assert!(TOPICS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    /// Each page has the layout that `summary` and `render` expect, and
    /// fits an 80-column terminal.
    #[test]
    fn pages_well_formed() {
        for (name, text) in TOPICS {
            let name = String::from_utf8_lossy(name);
            let mut lines = text.lines();
            assert!(lines.next().unwrap().starts_with("# "), "{name}: heading");
            assert_eq!(lines.next(), Some(""), "{name}");
            assert_eq!(lines.next(), Some("```text"), "{name}: synopsis");
            let s = summary(text, Code::Ticks);
            assert!(!s.is_empty() && s.len() <= 60, "{name}: summary {s:?}");
            for line in render(text, None).lines() {
                assert!(line.len() <= 79, "{name}: long line {line:?}");
            }
        }
    }

    /// Every page is in the user documentation.
    #[test]
    fn pages_in_docs() {
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
        let index = std::fs::read_to_string(docs.join("builtins.md")).unwrap();
        for f in std::fs::read_dir(docs.join("builtins")).unwrap() {
            let f = f.unwrap().file_name();
            let f = f.to_str().unwrap();
            assert!(index.contains(&format!("{{include}} builtins/{f}\n")), "{f}");
        }
    }

    #[test]
    fn inline_markup() {
        assert_eq!(inline("run `cd -` now", Code::Ticks), "run `cd -` now");
        assert_eq!(inline("run `cd -` now", Code::Bare), "run cd - now");
        assert_eq!(inline("a `` ` `` b", Code::Bare), "a ` b");
        assert_eq!(inline("**bold** and \\*", Code::Ticks), "bold and *");
        assert_eq!(
            inline("see [the docs](https://x.org/a) and [dir]", Code::Ticks),
            "see the docs and [dir]"
        );
        assert_eq!(inline("`[ expr ]`", Code::Bare), "[ expr ]");
        let code = Code::Paint { on: "<c>", base: "<b>" };
        assert_eq!(
            inline("run `cd -` **now**", code),
            "run <c>cd -\x1b[0m<b> \x1b[1mnow\x1b[22m"
        );
    }

    #[test]
    fn render_page() {
        let md =
            "# `x`\n\n```text\nx [-a]\n```\n\nDoes `x`.\n\n## `-a`\n\n`-a`\n: All,\n  `really`.\n\n```sh\nx -a\n```\n";
        assert_eq!(
            render(md, None),
            "x [-a]\n\nDoes `x`.\n\n-a\n\n-a\n    All,\n    `really`.\n\n    x -a\n"
        );
        assert_eq!(summary(md, Code::Ticks), "Does `x`.");
    }

    /// With colours: headings, code spans, terms in bold, the command
    /// names of the synopsis, and shell code highlighted.
    #[test]
    fn render_in_colour() {
        let p = Paint {
            heading: "<h>".into(),
            code: "<c>".into(),
            command: "<x>".into(),
            colors: Colors::default(),
        };
        let md = "# `x`\n\n```text\nx [-a]\n  [-b]\n```\n\nDoes `x`.\n\n## The `-a`\n\n`-a`\n: All.\n\n```sh\nif x -a; then\n  echo \"$y\"\nfi\n```\n";
        let r = "\x1b[0m";
        assert_eq!(
            render(md, Some(&p)).replace(r, "<0>"),
            concat!(
                "<x>x<0> [-a]\n  [-b]\n\nDoes <c>x<0>.\n\n<h>The -a<0>\n\n\x1b[1m<c>-a<0>\x1b[1m<0>\n    All.\n\n",
                "    \x1b[1;34mif<0> \x1b[32mx<0> -a\x1b[1m;<0> \x1b[1;34mthen<0>\n",
                "      \x1b[32mecho<0> \x1b[33m\"<0>\x1b[36m$y<0>\x1b[33m\"<0>\n",
                "    \x1b[1;34mfi<0>\n",
            )
        );
        assert_eq!(summary(md, Paint::code(Some(&p), "", Code::Ticks)), "Does <c>x\x1b[0m.");
    }
}
