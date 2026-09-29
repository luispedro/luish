// The engine of the completers: a command is described by a spec, a map,
// and `complete(spec, words, i)` completes the word `words[i]` of it.
//
// The fields of a spec, the format of its option tables and the kinds are
// documented in docs/plugins.md ("Reusing std's completion engine"), as
// other plugins use them (with `import "@std/completion/lib"`): new fields
// and kinds can be added, but those there must keep their meaning. Two
// fields are internal: `line`, the whole command line (for the kinds of a
// subcommand, whose `words` start at its name), and `hide`, the options
// that are valid in a mode but not offered.

import "kinds" as kinds;

// The options of a table: for each, `names`, `desc`, `arg` (whether it
// takes a value), `argname` (the name of its value), `optional` (whether
// that value is optional, after `=`) and `eq` (whether it was written
// after `=`), and `index`, each name's option.
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
        let o = #{names: [], desc: desc, arg: false, argname: "", optional: false, eq: false};
        for t in spec.split() {
            if !t.starts_with("-") {
                o.arg = true;    // `-n NUM`
                o.argname += t;
                continue;
            }
            let b = t.index_of("[=");
            if b > 0 {
                o.optional = true;
                o.argname += t.sub_string(b);
                t = t.sub_string(0, b);
            }
            let eq = t.index_of("=");
            if eq > 0 {
                o.arg = true;
                o.eq = true;
                o.argname += t.sub_string(eq);
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

// The subcommands of a table: for each, `names` and `desc`.
fn parse_commands(table) {
    let out = [];
    for line in table.split("\n") {
        line.trim();
        if line == "" {
            continue;
        }
        let gap = line.index_of("  ");
        let names = if gap < 0 { line } else { line.sub_string(0, gap) };
        let desc = if gap < 0 { "" } else { line.sub_string(gap) };
        desc.trim();
        names.replace(",", " ");
        out.push(#{names: names.split(), desc: desc});
    }
    out
}

// The candidate for one name of an option (`sd`: the command's long
// options start with one `-`; `strict`: see `strict_eq`).
fn candidate(o, name, sd, strict) {
    let long = name.starts_with("--") || sd;
    if long && o.optional {
        #{value: name, desc: o.desc, suffix: ""}
    } else if o.arg && ((name.starts_with("--") && !strict) || ((sd || strict) && o.eq)) {
        #{value: name + "=", desc: o.desc, suffix: ""}
    } else {
        #{value: name, desc: o.desc}
    }
}

// The options, for a word that starts with `-`: the long names for `--`,
// and otherwise the short names, and the long ones of the options that
// have no short name (so that each option shows once). All of them for a
// command whose long options start with one `-`.
fn options(t, cur, sd, strict) {
    let out = [];
    let long = cur.starts_with("--");
    for o in t.opts {
        if o.hidden == true {
            continue;
        }
        let short = o.names.filter(|n| !n.starts_with("--"));
        let names = if long {
            o.names.filter(|n| n.starts_with("--"))
        } else if short.is_empty() || sd {
            o.names
        } else {
            short
        };
        for n in names {
            out.push(candidate(o, n, sd, strict));
        }
    }
    out
}

// The note `[KEY: VALUE]` in a description, as `[the description without
// it, VALUE]` (VALUE is () without it).
fn note(desc, key) {
    let k = desc.index_of("[" + key + ": ");
    if k < 0 {
        return [desc, ()];
    }
    let rest = desc.sub_string(k + key.len() + 3);
    let end = rest.index_of("]");
    if end < 0 {
        return [desc, ()];
    }
    let v = rest.sub_string(0, end);
    let d = desc.sub_string(0, k) + rest.sub_string(end + 1);
    d.trim();
    [d, v]
}

// The output of `-h` of the command `path` (a program and its
// subcommands, separated by spaces), which clap doesn't wrap with
// COLUMNS=400, or "" if a word of it doesn't look like the name of a
// command (the words come from the command line).
fn help_of(path) {
    let words = path.split(" ");
    for w in words {
        if w == "" || w.starts_with("-") {
            return "";
        }
        for c in w.chars() {
            if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_'
                || c == '.') {
                return "";
            }
        }
    }
    sh::capture("COLUMNS=400 " + sh::quote(words) + " -h 2>/dev/null").out
}

// A spec from the `--help` output of a program, in the style of clap or
// GNU: the lines that start with `-` (after spaces) are its options, and
// those of a section whose heading has `command` in it are its
// subcommands. `[possible values: ...]` in a description gives the values
// of an option, and `[aliases: ...]` the other names of a subcommand.
fn help_spec(text) {
    let opts = "";
    let commands = "";
    let values = #{};
    let in_commands = false;
    for line in text.split("\n") {
        let t = line;
        t.trim();
        if t == "" {
            continue;
        }
        if !line.starts_with(" ") {
            in_commands = t.ends_with(":") && t.to_lower().contains("command");
            continue;
        }
        let gap = t.index_of("  ");
        let head = if gap < 0 { t } else { t.sub_string(0, gap) };
        let desc = if gap < 0 { "" } else { t.sub_string(gap) };
        desc.trim();
        head.replace("...", "");
        let pv = note(desc, "possible values");
        let al = note(pv[0], "aliases");
        desc = note(note(note(al[0], "default")[0], "env")[0], "short aliases")[0];
        // The first sentence, without notes that are cut off.
        for end in [". ", " ["] {
            let k = desc.index_of(end);
            if k > 0 {
                desc = desc.sub_string(0, k);
            }
        }
        if desc.ends_with(".") {
            desc = desc.sub_string(0, desc.len() - 1);
        }
        if desc.len() > 1 && desc[0] >= 'A' && desc[0] <= 'Z' && desc[1] >= 'a' && desc[1] <= 'z' {
            desc = desc[0].to_lower() + desc.sub_string(1);
        }
        if t.starts_with("-") {
            opts += head + "  " + desc + "\n";
            if pv[1] != () {
                let n = head.split()[0];
                n.replace(",", "");
                values[n] = pv[1].split(",").map(|v| {
                    v.trim();
                    v
                });
            }
        } else if in_commands && gap > 0 && !head.starts_with(".") && !head.split(", ").some(|n| n.contains(" ")) {
            commands += head + (if al[1] == () { "" } else { ", " + al[1] }) + "  " + desc + "\n";
        }
    }
    let spec = #{opts: opts, values: values};
    if commands != "" {
        spec.commands = commands;
    }
    spec
}

// The option called `name`, or ().
fn option(t, name) {
    let k = t.index[name];
    if k == () { () } else { t.opts[k] }
}

// What a value of the option `o` completes to, after `prefix` (such as
// `--color=`), which the word starts with. `line` is the command line
// (from the command, for a subcommand).
fn value(spec, o, line, cur, prefix) {
    let v = ();
    let values = spec.values ?? #{};
    for n in o.names {
        v = values[n];
        if v != () {
            break;
        }
    }
    if v == () && spec.guess_values == true {
        let n = o.argname.to_upper();
        v = if n.contains("FILE") || n.contains("PATH") {
            "files"
        } else if n.contains("DIR") {
            "dirs"
        } else {
            "none"
        };
    }
    let rest = cur.sub_string(prefix.len());
    let r = kinds::complete(v ?? "files", rest, line);
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
fn bundle(spec, t, line, cur) {
    let seen = [];
    let k = 1;
    for c in cur.sub_string(1).chars() {
        let o = option(t, "-" + c);
        if o == () {
            return [];
        }
        k += 1;
        if o.arg {
            return if k == cur.len() { [cur] } else { value(spec, o, line, cur, cur.sub_string(0, k)) };
        }
        seen += o.names;
    }
    let out = [];
    for o in t.opts {
        if o.hidden == true {
            continue;
        }
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
// aren't options, the names of the options given before it, and for a
// command with subcommands (`stop`), the index of the subcommand (or ()).
// The words that start with one of `skip` are left out.
fn scan(t, words, i, sd, stop, skip) {
    let s = #{expect: (), ended: false, pos: 0, given: [], cmd: ()};
    for j in 1..i {
        let w = words[j];
        if s.expect != () {
            s.expect = ();
        } else if skip.some(|p| w.starts_with(p)) {
            continue;
        } else if s.ended || w == "-" || !w.starts_with("-") {
            if stop && !s.ended {
                s.cmd = j;
                break;
            }
            s.pos += 1;
        } else if w == "--" {
            s.ended = true;
        } else if w.starts_with("--") || sd || option(t, w) != () {
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

// The spec `child` (of a subcommand or a mode) with what it gets from its
// command `parent`: the common options, the values of options and
// `single_dash`.
fn inherit(parent, child) {
    let c = child;
    c.common = (child.common ?? "") + "\n" + (parent.common ?? "");
    let v = parent.values ?? #{};
    v.mixin(child.values ?? #{});
    c.values = v;
    for f in ["single_dash", "strict_eq", "guess_values"] {
        if c[f] == () {
            c[f] = parent[f];
        }
    }
    c
}

// The spec of the subcommand `name` of `spec`.
fn subcommand(spec, name) {
    let subs = spec.subs ?? #{};
    for c in parse_commands(spec.commands) {
        if name in c.names {
            name = c.names[0];
            break;
        }
    }
    let sub = subs[name];
    if sub == () && spec.sub_spec != () {
        let colon = spec.sub_spec.index_of(":");
        let m = spec.sub_spec.sub_string(0, colon);
        import m as specs;
        sub = specs::sub_spec(spec.sub_spec.sub_string(colon + 1), name);
    }
    inherit(spec, sub ?? #{})
}

fn complete(spec, words, i) {
    // The words after the cursor don't matter.
    let words = words.extract(0, i + 1);
    let cur = words[i];
    // The whole command line, for kinds (a subcommand's `words` start at
    // its name).
    let line = spec.line ?? words;
    let sd = spec.single_dash ?? false;
    let strict = spec.strict_eq ?? false;
    let t = parse((spec.opts ?? "") + "\n" + (spec.common ?? ""));
    let skip = spec.skip ?? #{};
    for p in skip.keys() {
        if cur.starts_with(p) {
            let r = kinds::complete(skip[p], cur.sub_string(p.len()), line);
            return if type_of(r) == "map" { r.prefix = p + r.prefix; r } else { #{prefix: p, candidates: r} };
        }
    }
    // The options of the command in a mode's spec (`hide`) are valid, but
    // not offered.
    for n in spec.hide ?? [] {
        let k = t.index[n];
        if k != () {
            t.opts[k].hidden = true;
        }
    }
    let s = scan(t, words, i, sd, spec.commands != (), skip.keys());
    if spec.modes != () {
        // (Also in the word being completed, as `-S` in `pacman -Sy`, but
        // not in `-S` alone, which is completed as an option.)
        let given = s.given;
        if cur.len() > 2 && cur.starts_with("-") && !cur.starts_with("--") && !sd {
            for c in cur.sub_string(1).chars() {
                given.push("-" + c);
            }
        }
        for n in given {
            let m = spec.modes[n];
            if m != () {
                let m = inherit(spec, m);
                m.common += "\n" + (spec.opts ?? "");
                m.line = spec.line;
                m.hide = spec.modes.keys();
                return complete(m, words, i);
            }
        }
    }
    if s.cmd != () {
        let sub = subcommand(spec, words[s.cmd]);
        sub.line = line;
        return complete(sub, words.extract(s.cmd), i - s.cmd);
    }
    if s.expect != () {
        return value(spec, s.expect, line, cur, "");
    }
    // (A word `-` alone is an argument, standard input, for a command
    // without options.)
    if !s.ended && cur.starts_with("-") && !t.opts.is_empty() {
        // (A word of two letters or more can be one option, as `-nv` for
        // wget.)
        if !cur.starts_with("--") && !sd {
            return if cur.len() <= 2 || option(t, cur) != () {
                options(t, cur, sd, strict)
            } else {
                bundle(spec, t, line, cur)
            };
        }
        let eq = cur.index_of("=");
        if eq < 0 {
            return options(t, cur, sd, strict);
        }
        let o = option(t, cur.sub_string(0, eq));
        if o == () || !(o.arg || o.optional) {
            return [];
        }
        return value(spec, o, line, cur, cur.sub_string(0, eq + 1));
    }
    if spec.commands != () && !s.ended {
        return parse_commands(spec.commands).map(|c| #{value: c.names[0], desc: c.desc});
    }
    let a = arg_kinds(spec, s.given);
    kinds::complete(a[if s.pos < a.len() { s.pos } else { a.len() - 1 }], cur, line)
}
