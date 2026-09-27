// The engine of the completers: a command is described by a spec, a map,
// and `complete(spec, words, i)` completes the word `words[i]` of it.
//
// A spec has these fields, all optional:
//
// - `opts`: the options, as a table in the style of `--help`: one line per
//   option, with its names (`-a, --all`), then at least two spaces and its
//   description. `--name=ARG` or `-n ARG` means that the option takes a
//   value (with all its names), and `--name[=ARG]` that its long name can
//   take one after `=`.
// - `values`: what the values of options complete to, by one of the
//   option's names: a kind (see `kinds.rhai`) or an array of candidates.
//   By default, filenames.
// - `args`: the kinds of the arguments that aren't options, in order; the
//   last one repeats. By default, `["files"]`.
// - `args_if`: the `args` to use instead when an option is given, by one
//   of the option's names (such as `grep -e PATTERN`, after which the
//   first argument is a file).

import "kinds" as kinds;

// The options of a table: for each, `names`, `desc`, `arg` (whether it
// takes a value) and `optional` (whether that value is optional, after
// `=`), and `index`, each name's option.
fn parse(table) {
    let opts = [];
    let index = #{};
    for line in table.split("\n") {
        line.trim();
        if line == "" {
            continue;
        }
        let gap = line.index_of("  ");
        let spec = if gap < 0 { line } else { line.sub_string(0, gap) };
        let desc = if gap < 0 { "" } else { line.sub_string(gap) };
        desc.trim();
        spec.replace(",", " ");
        let o = #{names: [], desc: desc, arg: false, optional: false};
        for t in spec.split() {
            if !t.starts_with("-") {
                o.arg = true;    // `-n NUM`
                continue;
            }
            let b = t.index_of("[=");
            if b > 0 {
                o.optional = true;
                t = t.sub_string(0, b);
            }
            let eq = t.index_of("=");
            if eq > 0 {
                o.arg = true;
                t = t.sub_string(0, eq);
            }
            o.names.push(t);
        }
        for n in o.names {
            index[n] = opts.len();
        }
        opts.push(o);
    }
    #{opts: opts, index: index}
}

// The candidate for one name of an option.
fn candidate(o, name) {
    if name.starts_with("--") && o.arg && !o.optional {
        #{value: name + "=", desc: o.desc, suffix: ""}
    } else if name.starts_with("--") && o.optional {
        #{value: name, desc: o.desc, suffix: ""}
    } else {
        #{value: name, desc: o.desc}
    }
}

// The options, for a word that starts with `-`: the long names for `--`,
// and otherwise the short names, and the long ones of the options that
// have no short name (so that each option shows once).
fn options(t, cur) {
    let out = [];
    let long = cur.starts_with("--");
    for o in t.opts {
        let short = o.names.filter(|n| !n.starts_with("--"));
        let names = if long {
            o.names.filter(|n| n.starts_with("--"))
        } else if short.is_empty() {
            o.names
        } else {
            short
        };
        for n in names {
            out.push(candidate(o, n));
        }
    }
    out
}

// The option called `name`, or ().
fn option(t, name) {
    let k = t.index[name];
    if k == () { () } else { t.opts[k] }
}

// What a value of the option `o` completes to, after `prefix` (such as
// `--color=`), which the word starts with.
fn value(spec, o, words, cur, prefix) {
    let v = ();
    let values = spec.values ?? #{};
    for n in o.names {
        v = values[n];
        if v != () {
            break;
        }
    }
    let rest = cur.sub_string(prefix.len());
    let r = kinds::complete(v ?? "files", rest, words);
    if prefix == "" {
        return r;
    }
    if r == () {
        // luish completes filenames after `--opt=` itself (it splits the
        // word at `=`), but not after `-f` in `-fFILE`.
        return if prefix.starts_with("--") { () } else { kinds::files(rest, prefix) };
    }
    if type_of(r) == "map" {
        r.prefix = prefix + r.prefix;
        r
    } else {
        #{prefix: prefix, candidates: r}
    }
}

// A word of short options, such as `-la`: the options that can follow, or
// the value of the last one that takes a value (`-n5`, `-fFILE`).
fn bundle(spec, t, words, cur) {
    let seen = [];
    let k = 1;
    for c in cur.sub_string(1).chars() {
        let o = option(t, "-" + c);
        if o == () {
            return [];
        }
        k += 1;
        if o.arg {
            return if k == cur.len() { [cur] } else { value(spec, o, words, cur, cur.sub_string(0, k)) };
        }
        seen += o.names;
    }
    let out = [];
    for o in t.opts {
        for n in o.names {
            if n.len() == 2 && n != "--" && !(n in seen) {
                out.push(#{value: n.sub_string(1), desc: o.desc, suffix: if o.arg { " " } else { "" }});
            }
        }
    }
    #{prefix: cur, candidates: out}
}

// The kinds of the arguments that aren't options, given the options on
// the command line.
fn arg_kinds(spec, given) {
    let alt = spec.args_if ?? #{};
    for n in alt.keys() {
        if n in given {
            return alt[n];
        }
    }
    spec.args ?? ["files"]
}

// Where the word `words[i]` is: the option whose value it is (or ()),
// whether it comes after `--`, the number of arguments before it that
// aren't options, and the names of the options given before it.
fn scan(t, words, i) {
    let s = #{expect: (), ended: false, pos: 0, given: []};
    for j in 1..i {
        let w = words[j];
        if s.expect != () {
            s.expect = ();
        } else if s.ended || w == "-" || !w.starts_with("-") {
            s.pos += 1;
        } else if w == "--" {
            s.ended = true;
        } else if w.starts_with("--") {
            let eq = w.index_of("=");
            let o = option(t, if eq < 0 { w } else { w.sub_string(0, eq) });
            if o != () {
                s.given += o.names;
                if o.arg && !o.optional && eq < 0 {
                    s.expect = o;
                }
            }
        } else {
            let k = 1;
            for c in w.sub_string(1).chars() {
                let o = option(t, "-" + c);
                if o == () {
                    break;
                }
                s.given += o.names;
                k += 1;
                if o.arg {
                    if k == w.len() {
                        s.expect = o;
                    }
                    break;
                }
            }
        }
    }
    s
}

fn complete(spec, words, i) {
    // The words after the cursor don't matter.
    let words = words.extract(0, i + 1);
    let cur = words[i];
    let t = parse(spec.opts ?? "");
    let s = scan(t, words, i);
    if s.expect != () {
        return value(spec, s.expect, words, cur, "");
    }
    // (A word `-` alone is an argument, standard input, for a command
    // without options.)
    if !s.ended && cur.starts_with("-") && !t.opts.is_empty() {
        if !cur.starts_with("--") {
            return if cur.len() <= 2 { options(t, cur) } else { bundle(spec, t, words, cur) };
        }
        let eq = cur.index_of("=");
        if eq < 0 {
            return options(t, cur);
        }
        let o = option(t, cur.sub_string(0, eq));
        if o == () || !(o.arg || o.optional) {
            return [];
        }
        return value(spec, o, words, cur, cur.sub_string(0, eq + 1));
    }
    let a = arg_kinds(spec, s.given);
    kinds::complete(a[if s.pos < a.len() { s.pos } else { a.len() - 1 }], cur, words)
}
