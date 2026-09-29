// Completion by programs that complete themselves: those built with Cobra
// (gh, docker, kubectl ...) and nix. The completers are `cobra` and `nix`,
// which extension.rhai registers for these programs.

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
    let args = sh::quote(words.extract(1, i));
    let r = sh::capture(sh::quote(words[0]) + " __complete " + args + " 2>/dev/null");
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
    let args = sh::quote(words.extract(1, i));
    let r = sh::capture(`NIX_GET_COMPLETIONS=${i} ${sh::quote(words[0])} ${args} 2>/dev/null`);
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
