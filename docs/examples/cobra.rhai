// Completion for programs built with Cobra (such as gh, docker, kubectl and
// helm), which complete their own arguments: `prog __complete ARGS... WORD`
// prints one candidate per line (a tab, then its description), then a line
// `:N`, where N is a set of flags.

fn cobra(words, i) {
    // The words up to the one being completed (the words after the cursor
    // are left out).
    let r = sh::capture([words[0], "__complete"] + words.extract(1, i));
    let lines = r.out.split("\n");
    let last = lines.pop();
    if r.status != 0 || !last.starts_with(":") {
        return ();
    }
    let flags = parse_int(last.sub_string(1));
    if (flags & 1) != 0 {
        return [];  // an error
    }
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
            c.suffix = "";  // no space after it
        }
        out.push(c);
    }
    // Filenames, unless the program said not to.
    if out.is_empty() && (flags & 4) == 0 {
        return ();
    }
    out
}

for prog in ["gh", "docker", "kubectl", "helm"] {
    sh::completer(prog, Fn("cobra"));
}
