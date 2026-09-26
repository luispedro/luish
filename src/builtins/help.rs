//! `help`: shows the documentation of built-ins. The text is the Markdown
//! in `docs/builtins/`, which the user documentation includes too, shown
//! as plain text.
//!
//! `help` is a built-in only in interactive shells (see `INTERACTIVE` in
//! `mod.rs`); `__luish_internal help` is the same command everywhere.

use crate::shell::{ExecResult, Shell};

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
    (b"break", page!("break")),
    (b"cd", page!("cd")),
    (b"chdir", page!("cd")),
    (b"command", page!("command")),
    (b"continue", page!("continue")),
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
    (b"local", page!("local")),
    (b"plugin", page!("plugin")),
    (b"popd", page!("dirs")),
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
    (b"test", page!("test")),
    (b"times", page!("times")),
    (b"trap", page!("trap")),
    (b"true", page!("true")),
    (b"type", page!("type")),
    (b"ulimit", page!("ulimit")),
    (b"umask", page!("umask")),
    (b"unalias", page!("unalias")),
    (b"unset", page!("unset")),
    (b"unsetopt", page!("setopt")),
    (b"wait", page!("wait")),
];

/// `help`.
pub fn help(sh: &mut Shell, argv: &[Vec<u8>]) -> ExecResult {
    run(sh, &argv[0], &argv[1..])
}

/// Shows the help for each of `args`, or the list of topics without any.
/// `name` is the command, for error messages.
pub fn run(sh: &mut Shell, name: &[u8], args: &[Vec<u8>]) -> ExecResult {
    if args.is_empty() {
        return Ok(sh.out_status(index().as_bytes()));
    }
    let mut status = 0;
    for (i, a) in args.iter().enumerate() {
        match TOPICS.iter().find(|t| t.0 == a.as_slice()) {
            Some((_, text)) => {
                let sep = if i > 0 { "\n" } else { "" };
                sh.out(format!("{sep}{}", render(text)).as_bytes());
            }
            None => {
                sh.berr(name, format!("no help for {}", String::from_utf8_lossy(a)));
                status = 1;
            }
        }
    }
    Ok(status)
}

/// The list of topics, each with its summary.
fn index() -> String {
    let mut out = String::from("Built-in commands (`help NAME` shows more about one):\n\n");
    for (name, text) in TOPICS {
        out.push_str(&format!("  {:<18}{}\n", String::from_utf8_lossy(name), summary(text)));
    }
    out
}

/// The line after the synopsis.
fn summary(md: &str) -> String {
    let mut in_code = false;
    let mut seen_code = false;
    for line in md.lines().skip(1) {
        if line.starts_with("```") {
            in_code = !in_code;
            seen_code = true;
        } else if seen_code && !in_code && !line.is_empty() {
            return inline(line, true);
        }
    }
    String::new()
}

/// Renders Markdown as plain text for a terminal. Only what the pages use
/// is handled: the heading of the page (left out), code blocks (the first
/// one, the synopsis, flush left, the others indented), definition lists
/// (`: ` lines and their continuations indented), `**`, links and
/// backslash escapes. Code spans keep their backticks, except in headings
/// and the terms of definition lists.
fn render(md: &str) -> String {
    let mut out = String::new();
    let mut code: Option<&str> = None;
    let mut synopsis = true;
    let mut definition = false;
    let mut lines = md.lines().skip(1).skip_while(|l| l.is_empty()).peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("```") {
            code = match code {
                Some(_) => None,
                None => Some(if std::mem::take(&mut synopsis) { "" } else { "    " }),
            };
            continue;
        }
        if let Some(indent) = code {
            out.push_str(indent);
            out.push_str(line);
        } else if let Some(def) = line.strip_prefix(": ") {
            definition = true;
            out.push_str("    ");
            out.push_str(&inline(def, true));
        } else if definition && line.starts_with("  ") {
            out.push_str("  ");
            out.push_str(&inline(line, true));
        } else {
            definition = false;
            let heading = line
                .trim_start_matches('#')
                .strip_prefix(' ')
                .filter(|_| line.starts_with('#'));
            let term = lines.peek().is_some_and(|l| l.starts_with(": "));
            out.push_str(&inline(heading.unwrap_or(line), heading.is_none() && !term));
        }
        out.push('\n');
    }
    out
}

/// Renders the inline markup of a line: `**`, links and backslash escapes,
/// and code spans, which keep their backticks with `keep_code`.
fn inline(line: &str, keep_code: bool) -> String {
    let b = line.as_bytes();
    let mut out = Vec::with_capacity(b.len());
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
                if keep_code {
                    out.extend_from_slice(&b[i..j + n]);
                } else {
                    let mut span = &b[i + n..j];
                    if span.len() > 2 && span[0] == b' ' && span[span.len() - 1] == b' ' {
                        span = &span[1..span.len() - 1];
                    }
                    out.extend_from_slice(span);
                }
                i = j + n;
            }
            b'*' if b.get(i + 1) == Some(&b'*') => i += 2,
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
                        out.extend_from_slice(inline(&line[i + 1..e], keep_code).as_bytes());
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
            let s = summary(text);
            assert!(!s.is_empty() && s.len() <= 60, "{name}: summary {s:?}");
            for line in render(text).lines() {
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
        assert_eq!(inline("run `cd -` now", true), "run `cd -` now");
        assert_eq!(inline("run `cd -` now", false), "run cd - now");
        assert_eq!(inline("a `` ` `` b", false), "a ` b");
        assert_eq!(inline("**bold** and \\*", true), "bold and *");
        assert_eq!(
            inline("see [the docs](https://x.org/a) and [dir]", true),
            "see the docs and [dir]"
        );
        assert_eq!(inline("`[ expr ]`", false), "[ expr ]");
    }

    #[test]
    fn render_page() {
        let md =
            "# `x`\n\n```text\nx [-a]\n```\n\nDoes `x`.\n\n## `-a`\n\n`-a`\n: All,\n  `really`.\n\n```sh\nx -a\n```\n";
        assert_eq!(
            render(md),
            "x [-a]\n\nDoes `x`.\n\n-a\n\n-a\n    All,\n    `really`.\n\n    x -a\n"
        );
        assert_eq!(summary(md), "Does `x`.");
    }
}
