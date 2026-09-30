// Completion by programs that complete themselves: those built with Cobra
// (gh, docker, kubectl ...) or Click (Python), and nix. The completers are
// `cobra`, `click` and `nix`, which extension.rhai registers for these programs.

import "kinds" as kinds;

// The files whose names end with one of the extensions `exts` (without
// the dot), and directories.
fn with_ext(cur, exts) {
    let r = kinds::files(cur, "");
    r.candidates = r.candidates.filter(|c| type_of(c) == "map" || exts.some(|e| c.ends_with("." + e)));
    r
}

// Programs built with Cobra complete themselves: `PROG __complete ARGS...
// WORD` prints one candidate per line (then a tab and its description),
// then a line `:N`, where N is a set of flags (1: an error, 2: no space
// after the candidate, 4: no filenames if there are no candidates, 8: the
// candidates are extensions of filenames, 16: directories only). For a
// word `--flag=VALUE`, the candidates are the values.
fn cobra(words, i) {
    let r = sh::capture([words[0], "__complete"] + words.extract(1, i));
    let lines = r.out.split("\n");
    let last = lines.pop();
    if r.status != 0 || last == () || !last.starts_with(":") {
        return ();
    }
    let flags = parse_int(last.sub_string(1));
    if (flags & 1) != 0 {
        return [];
    }
    let cur = words[i];
    let out = [];
    for line in lines {
        if line == "" || line.starts_with("_activeHelp_") {
            continue;
        }
        let tab = line.index_of('\t');
        let c = if tab < 0 {
            #{value: line}
        } else {
            #{value: line.sub_string(0, tab), desc: line.sub_string(tab + 1)}
        };
        if (flags & 2) != 0 {
            c.suffix = "";
        }
        out.push(c);
    }
    if (flags & 8) != 0 {
        return with_ext(cur, out.map(|c| c.value));
    }
    if (flags & 16) != 0 {
        return kinds::dirs(cur);
    }
    if out.is_empty() && (flags & 4) == 0 {
        return ();
    }
    let eq = cur.index_of("=");
    if cur.starts_with("-") && eq > 0 {
        return #{prefix: cur.sub_string(0, eq + 1), candidates: out};
    }
    out
}

// Whether `s` can be part of the name of an environment variable.
fn is_var_name(s) {
    if s == "" {
        return false;
    }
    for c in s.chars() {
        if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_') {
            return false;
        }
    }
    true
}

// Programs built with Click (Python) complete themselves: with
// `_PROG_COMPLETE=fish_complete` (PROG is the name of the program in
// capitals, `-` as `_`), `COMP_WORDS` the command line and `COMP_CWORD` the
// word to complete, they print one candidate per line: `TYPE,VALUE`, then a
// tab and its description if it has one. TYPE is `plain`, or `file` or `dir`
// for a value that is a path (then VALUE is the word, and the completion is
// ours). For a word `--flag=VALUE`, the candidates are the values. A program
// that doesn't know the protocol runs as it would with no environment, so
// this is only for the programs registered as Click programs; what it
// prints then doesn't look like candidates and is ignored.
fn click(words, i) {
    let prog = words[0].split("/").pop();
    if !is_var_name(prog) {
        return ();
    }
    let name = prog;
    name.replace("-", "_");
    let envvar = "_" + name.to_upper() + "_COMPLETE";
    let cur = words[i];
    // (as fish gives them: the word to complete is in COMP_WORDS only if it isn't empty.)
    let line = sh::quote(words.extract(0, if cur == "" { i } else { i + 1 })) + (if cur == "" { " " } else { "" });
    let r = sh::capture(["env", envvar + "=fish_complete", "COMP_WORDS=" + line, "COMP_CWORD=" + cur, words[0]]);
    if r.status != 0 {
        return ();
    }
    let out = [];
    let kind = "plain";
    for l in r.out.split("\n") {
        if l == "" {
            continue;
        }
        let comma = l.index_of(",");
        let t = if comma < 0 { "" } else { l.sub_string(0, comma) };
        if t != "plain" && t != "file" && t != "dir" {
            return ();
        }
        kind = t;
        let rest = l.sub_string(comma + 1);
        let tab = rest.index_of('\t');
        let c = if tab < 0 {
            #{value: rest}
        } else {
            let desc = rest.sub_string(tab + 1);
            // (a description of several lines has `\n` in it.)
            let nl = desc.index_of("\\n");
            if nl >= 0 {
                desc = desc.sub_string(0, nl);
            }
            desc.trim();
            #{value: rest.sub_string(0, tab), desc: desc}
        };
        out.push(c);
    }
    if out.is_empty() {
        return ();
    }
    let eq = cur.index_of("=");
    let prefix = if cur.starts_with("-") && eq > 0 { cur.sub_string(0, eq + 1) } else { "" };
    if kind != "plain" {
        if prefix == "" && kind == "dir" {
            return kinds::dirs(cur);
        }
        return kinds::listing(cur.sub_string(prefix.len()), prefix, kind == "dir");
    }
    if prefix != "" {
        return #{prefix: prefix, candidates: out};
    }
    out
}

// A description of nix's, without its Markdown.
fn plain(desc) {
    loop {
        let close = desc.index_of("](");
        if close < 0 {
            break;
        }
        let open = desc.sub_string(0, close).index_of("[");
        if open < 0 {
            break;
        }
        let end = desc.index_of(")", close);
        // (nix cuts long descriptions, links too.)
        let rest = if end < 0 { "" } else { desc.sub_string(end + 1) };
        desc = desc.sub_string(0, open) + desc.sub_string(open + 1, close - open - 1) + rest;
    }
    desc.replace("*", "");
    desc.replace("`", "");
    desc
}

// nix completes itself: with `NIX_GET_COMPLETIONS=N` (the index of the word
// to complete), `nix ARGS...` prints the kind of completion (`normal`,
// `filenames`, or `attrs` for attribute paths, which go on), then one
// candidate per line, with a tab and its description.
fn nix(words, i) {
    let r = sh::capture(["env", `NIX_GET_COMPLETIONS=${i}`, words[0]] + words.extract(1, i));
    let lines = r.out.split("\n");
    if lines.is_empty() || lines[0] == "" {
        return ();
    }
    let kind = lines.shift();
    if kind == "filenames" {
        return ();
    }
    let out = [];
    for line in lines {
        if line == "" {
            continue;
        }
        let tab = line.index_of('\t');
        let c = if tab < 0 { #{value: line} } else { #{value: line.sub_string(0, tab)} };
        if tab >= 0 {
            let d = plain(line.sub_string(tab + 1));
            if d != "" {
                c.desc = d;
            }
        }
        if kind == "attrs" {
            c.suffix = "";
        }
        out.push(c);
    }
    out
}
