// What the values of options and the arguments of commands complete to,
// by kind: `complete(kind, cur, words)` completes the word `cur` (the
// value, without an option before it) of the command `words`.
//
// The kinds are listed in docs/extensions.md ("Reusing std's completion
// engine"). `MODULE:NAME` is `MODULE::kind(NAME, cur, words)`: one of
// std's modules, or `@SOURCE/PLUGIN/MODULE` in another plugin. An array is
// its own candidates.

fn home() {
    sh::getvar("HOME") ?? ""
}

// A path as the shell would expand a leading `~`.
fn tilde(p) {
    if p == "~" || p.starts_with("~/") { home() + p.sub_string(1) } else { p }
}

// The directory part of a word (up to its last `/`), and the rest.
fn split_dir(cur) {
    let slash = -1;
    let k = 0;
    for c in cur.chars() {
        if c == '/' {
            slash = k;
        }
        k += 1;
    }
    [cur.sub_string(0, slash + 1), cur.sub_string(slash + 1)]
}

// The names in the directory of `cur` (`dirs_only`: the directories), as
// a completion of the part after its last `/`, after `prefix` (such as
// `-f` in `tar -fFILE`). Names that start with `.` only if it does.
fn listing(cur, prefix, dirs_only) {
    let parts = split_dir(cur);
    let dir = parts[0];
    let path = tilde(if dir == "" { "." } else { dir });
    let out = [];
    for n in fs::list_dir(path) ?? [] {
        if n.starts_with(".") && !parts[1].starts_with(".") {
            continue;
        }
        let p = path + "/" + n;
        if fs::is_dir(p) {
            out.push(#{value: n + "/", suffix: ""});
        } else if !dirs_only {
            out.push(n);
        }
    }
    #{prefix: prefix + dir, candidates: out}
}

fn dirs(cur) {
    if cur.starts_with("~") && !cur.contains("/") {
        return ();   // `~user`: luish's own
    }
    listing(cur, "", true)
}

// Filenames, for a value that luish doesn't see as a filename, such as the
// `FILE` of `-fFILE`.
fn files(cur, prefix) {
    listing(cur, prefix, false)
}

// The first fields of the lines of a file such as `/etc/passwd`.
fn names_in(path) {
    let out = [];
    for line in (fs::read_file(path) ?? "").split("\n") {
        let c = line.index_of(":");
        if c > 0 && !line.starts_with("#") {
            out.push(line.sub_string(0, c));
        }
    }
    out
}

fn users() {
    names_in("/etc/passwd")
}

fn groups() {
    names_in("/etc/group")
}

// `USER`, or `USER:GROUP` once the word has a `:` (or `.`, as chown takes
// too).
fn owner(cur) {
    let c = cur.index_of(":");
    if c < 0 {
        return users();
    }
    #{prefix: cur.sub_string(0, c + 1), candidates: groups()}
}

fn mode() {
    [
        #{value: "u+x", desc: "let the owner run it"},
        #{value: "a+x", desc: "let everyone run it"},
        #{value: "+x", desc: "make it executable, as the umask allows"},
        #{value: "-x", desc: "make it not executable"},
        #{value: "go-w", desc: "let only the owner write to it"},
        #{value: "go-rwx", desc: "keep it private to the owner"},
        #{value: "a+r", desc: "let everyone read it"},
        #{value: "u+w", desc: "let the owner write to it"},
        #{value: "644", desc: "rw-r--r--"},
        #{value: "755", desc: "rwxr-xr-x"},
        #{value: "600", desc: "rw-------"},
        #{value: "700", desc: "rwx------"},
        #{value: "666", desc: "rw-rw-rw-"},
        #{value: "777", desc: "rwxrwxrwx"},
    ]
}

fn signals() {
    [
        "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1", "SEGV", "USR2", "PIPE", "ALRM",
        "TERM", "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU", "URG", "XCPU", "XFSZ", "VTALRM", "PROF",
        "WINCH", "IO", "PWR", "SYS",
    ]
}

// The running processes: their names (`names`), or their IDs, with their
// names as descriptions.
fn processes(names) {
    let out = [];
    let seen = #{};
    for p in fs::list_dir("/proc") ?? [] {
        if !(p[0] >= '0' && p[0] <= '9') {
            continue;
        }
        let comm = fs::read_file(`/proc/${p}/comm`);
        if comm == () {
            continue;
        }
        comm.trim();
        if !names {
            out.push(#{value: p, desc: comm});
        } else if !seen.contains(comm) {
            seen[comm] = true;
            out.push(comm);
        }
    }
    out
}

// A field of /proc/mounts, in which the kernel writes a space, tab, newline
// or backslash as `\040`, `\011`, `\012` or `\134`. `\134` is decoded last,
// so that the backslash it makes doesn't start another escape.
fn unescape_mount(s) {
    if s.contains("\\") {
        for e in [["\\040", " "], ["\\011", "\t"], ["\\012", "\n"], ["\\134", "\\"]] {
            s.replace(e[0], e[1]);
        }
    }
    s
}

// The mounted file systems, from /proc/mounts (or `text` in its format), as
// `#{source, point, type}`, decoded.
fn mount_table(text) {
    let out = [];
    for line in text.split("\n") {
        let f = line.split(" ");
        if f.len() >= 3 {
            out.push(#{source: unescape_mount(f[0]), point: unescape_mount(f[1]), type: unescape_mount(f[2])});
        }
    }
    out
}

fn mount_table() {
    mount_table(fs::read_file("/proc/mounts") ?? "")
}

// Whether `name` matches a pattern with `*` in it (as in ssh's `Include`).
fn glob_match(pat, name) {
    let parts = pat.split("*");
    if !name.starts_with(parts[0]) || !name.ends_with(parts[-1]) {
        return false;
    }
    let at = parts[0].len();
    for p in parts.extract(1, parts.len() - 2) {
        let k = name.index_of(p, at);
        if k < 0 {
            return false;
        }
        at = k + p.len();
    }
    at <= name.len() - parts[-1].len()
}

// The files an ssh `Include` names: relative to `~/.ssh`, with `*` in its
// last component.
fn include(arg) {
    let p = tilde(arg);
    if !p.starts_with("/") {
        p = home() + "/.ssh/" + p;
    }
    if !p.contains("*") {
        return [p];
    }
    let parts = split_dir(p);
    let out = [];
    for n in fs::list_dir(parts[0]) ?? [] {
        if glob_match(parts[1], n) {
            out.push(parts[0] + n);
        }
    }
    out
}

// The hosts of an ssh config file and those it includes, without
// patterns.
fn ssh_hosts(path, depth) {
    let out = [];
    if depth > 8 {
        return out;
    }
    for line in (fs::read_file(path) ?? "").split("\n") {
        let w = line.split();
        if w.len() < 2 {
            continue;
        }
        let key = w[0].to_lower();
        if key == "host" {
            out += w.extract(1).filter(|h| !h.contains("*") && !h.contains("?") && !h.starts_with("!"));
        } else if key == "include" {
            for arg in w.extract(1) {
                for f in include(arg) {
                    out += ssh_hosts(f, depth + 1);
                }
            }
        }
    }
    out
}

fn host_names() {
    let out = [];
    for h in ssh_hosts(home() + "/.ssh/config", 0) {
        if !(h in out) {
            out.push(h);
        }
    }
    for line in (fs::read_file("/etc/hosts") ?? "").split("\n") {
        let hash = line.index_of("#");
        if hash >= 0 {
            line = line.sub_string(0, hash);
        }
        for h in line.split().extract(1) {
            if !(h in out) {
                out.push(h);
            }
        }
    }
    out
}

// Hosts, after `USER@` if the word has it; `suffix` follows a host (`:`
// for scp).
fn hosts(cur, suffix) {
    let at = cur.index_of("@");
    let out = host_names().map(|h| #{value: h, suffix: suffix});
    if at < 0 { out } else { #{prefix: cur.sub_string(0, at + 1), candidates: out} }
}

// scp's and rsync's arguments: `HOST:PATH` (nothing is completed on the
// host), or local files and `HOST:`.
fn remote(cur) {
    let colon = cur.index_of(":");
    let slash = cur.index_of("/");
    if colon >= 0 && (slash < 0 || colon < slash) {
        return [];
    }
    if cur.contains("@") {
        return hosts(cur, ":");
    }
    if cur.contains("/") || cur.starts_with(".") || cur.starts_with("~") {
        return files(cur, "");
    }
    let r = files(cur, "");
    r.candidates += hosts(cur, ":");
    r
}

// The directories of the manual.
fn man_dirs() {
    let path = sh::getvar("MANPATH") ?? "";
    if path == "" || path.starts_with(":") || path.ends_with(":") || path.contains("::") {
        let r = sh::capture(["manpath", "-q"]);
        let found = if r.status == 0 { r.out } else { "/usr/local/share/man:/usr/share/man" };
        path = if path == "" { found } else { path + ":" + found };
    }
    path.split(":").filter(|d| d != "")
}

// A page's name, from its file's (`ls.1.gz` is `ls`).
fn page_name(f) {
    for z in [".gz", ".xz", ".bz2", ".zst", ".lzma", ".Z"] {
        if f.ends_with(z) {
            f = f.sub_string(0, f.len() - z.len());
            break;
        }
    }
    let dot = -1;
    let k = 0;
    for c in f.chars() {
        if c == '.' {
            dot = k;
        }
        k += 1;
    }
    if dot > 0 { f.sub_string(0, dot) } else { f }
}

// The manual's pages that start with `cur`, in section `section` (such
// as `3`, or `3p`), or in all of them for ().
fn man_pages(cur, section) {
    let out = [];
    let seen = #{};
    for d in man_dirs() {
        for s in fs::list_dir(d) ?? [] {
            if !s.starts_with("man") || (section != () && s.sub_string(3) != section.sub_string(0, 1)) {
                continue;
            }
            for f in fs::list_dir(d + "/" + s) ?? [] {
                if !f.starts_with(cur) {
                    continue;
                }
                let n = page_name(f);
                if section != () && section.len() > 1 && !f.contains("." + section) {
                    continue;
                }
                if !seen.contains(n) {
                    seen[n] = true;
                    out.push(n);
                }
            }
        }
    }
    out
}

fn sections() {
    [
        #{value: "1", desc: "commands"},
        #{value: "2", desc: "system calls"},
        #{value: "3", desc: "library functions"},
        #{value: "4", desc: "special files"},
        #{value: "5", desc: "file formats"},
        #{value: "6", desc: "games"},
        #{value: "7", desc: "overviews and conventions"},
        #{value: "8", desc: "administration commands"},
    ]
}

// The section `man` was given (the argument before the page), or ().
fn man_section(words) {
    let prev = ();
    for w in words.extract(1, words.len() - 2) {
        if !w.starts_with("-") {
            prev = w;
        }
    }
    if prev != () && prev.len() <= 3 && prev[0] >= '0' && prev[0] <= '9' { prev } else { () }
}

// The value of an option in `words` (`-f FILE`, `-fFILE`, `--file=FILE`
// or `--file FILE`), by its names, or ().
fn option_value(words, short, long) {
    let v = ();
    let n = words.len() - 1;   // not the word being completed
    for j in 1..n {
        let w = words[j];
        if w == short || w == long {
            if j + 1 < n {
                v = words[j + 1];
            }
        } else if short != "" && w.starts_with(short) && !w.starts_with("--") {
            v = w.sub_string(short.len());
        } else if long != "" && w.starts_with(long + "=") {
            v = w.sub_string(long.len() + 1);
        }
    }
    v
}

// make's targets, from the makefile that `-f` names or the default ones,
// in the directory `-C` names. Targets that are patterns or special
// (`.PHONY`) or have variables are left out.
fn targets(words) {
    let dir = tilde(option_value(words, "-C", "--directory") ?? ".");
    let f = option_value(words, "-f", "--file");
    let files = if f != () {
        [if f.starts_with("/") { tilde(f) } else { dir + "/" + f }]
    } else {
        ["GNUmakefile", "makefile", "Makefile"].map(|m| dir + "/" + m)
    };
    let text = ();
    for m in files {
        text = fs::read_file(m);
        if text != () {
            break;
        }
    }
    let out = [];
    for line in (text ?? "").split("\n") {
        if line == "" || line.starts_with("\t") || line.starts_with("#") || line.starts_with(".") {
            continue;
        }
        let c = line.index_of(":");
        if c <= 0 || line.sub_string(c).starts_with(":=") || line.sub_string(c).starts_with("::=") {
            continue;
        }
        let head = line.sub_string(0, c);
        if head.contains("=") || head.contains("$") || head.contains("%") {
            continue;
        }
        for t in head.split() {
            if !(t in out) {
                out.push(t);
            }
        }
    }
    out
}

// The archive of a tar command line: the value of `-f`, also in a word of
// options (`-xzf ARCHIVE`, `-xzfARCHIVE`) and in the old style, where the
// first word has the options without `-` (`tar xzf ARCHIVE`).
fn tar_archive(words) {
    let v = ();
    let n = words.len() - 1;
    for j in 1..n {
        let w = words[j];
        if w == "--file" {
            if j + 1 < n {
                v = words[j + 1];
            }
        } else if w.starts_with("--file=") {
            v = w.sub_string(7);
        } else if (w.starts_with("-") && !w.starts_with("--")) || j == 1 {
            let f = w.index_of("f");
            if f < 0 {
                continue;
            }
            if f < w.len() - 1 {
                v = w.sub_string(f + 1);
            } else if j + 1 < n {
                v = words[j + 1];
            }
        }
    }
    v
}

// The files in the archive of `tar -f ARCHIVE`, for extracting or listing
// some of them, one directory at a time; () for filenames otherwise.
fn members(cur, words) {
    let mode = "";
    for w in words.extract(1, words.len() - 2) {
        if w.starts_with("--") {
            if w in ["--extract", "--get", "--list"] {
                mode = "x";
            }
        } else if w.starts_with("-") || w == words[1] {
            // Options, and the old style (`tar xf ARCHIVE`).
            if w.contains("x") || w.contains("t") {
                mode = "x";
            }
        }
    }
    let archive = tar_archive(words);
    if mode != "x" || archive == () || !fs::is_file(tilde(archive)) {
        return ();
    }
    let r = sh::capture(["tar", "-tf", tilde(archive)]);
    if r.status != 0 {
        return [];
    }
    let out = [];
    let seen = #{};
    for m in r.out.split("\n") {
        if !m.starts_with(cur) || m == cur {
            continue;
        }
        let slash = m.sub_string(cur.len()).index_of("/");
        let v = if slash < 0 || cur.len() + slash + 1 == m.len() { m } else { m.sub_string(0, cur.len() + slash + 1) };
        if !seen.contains(v) {
            seen[v] = true;
            out.push(if v.ends_with("/") { #{value: v, suffix: ""} } else { v });
        }
    }
    out
}

// dd's operands, and their values after `=`.
fn dd(cur) {
    let eq = cur.index_of("=");
    if eq < 0 {
        return [
            #{value: "if=", desc: "read from FILE instead of standard input", suffix: ""},
            #{value: "of=", desc: "write to FILE instead of standard output", suffix: ""},
            #{value: "bs=", desc: "read and write up to BYTES bytes at a time", suffix: ""},
            #{value: "ibs=", desc: "read up to BYTES bytes at a time", suffix: ""},
            #{value: "obs=", desc: "write BYTES bytes at a time", suffix: ""},
            #{value: "cbs=", desc: "convert BYTES bytes at a time", suffix: ""},
            #{value: "count=", desc: "copy only N input blocks", suffix: ""},
            #{value: "skip=", desc: "skip N ibs-sized input blocks", suffix: ""},
            #{value: "seek=", desc: "skip N obs-sized output blocks", suffix: ""},
            #{value: "conv=", desc: "convert the file as per the comma separated symbol list", suffix: ""},
            #{value: "iflag=", desc: "read as per the comma separated symbol list", suffix: ""},
            #{value: "oflag=", desc: "write as per the comma separated symbol list", suffix: ""},
            #{value: "status=", desc: "the level of information to print to standard error", suffix: ""},
        ];
    }
    let key = cur.sub_string(0, eq);
    let rest = cur.sub_string(eq + 1);
    if key == "if" || key == "of" {
        return files(rest, cur.sub_string(0, eq + 1));
    }
    let values = switch key {
        "conv" => [
            "ascii", "ebcdic", "ibm", "block", "unblock", "lcase", "ucase", "sparse", "swab", "sync", "excl",
            "nocreat", "notrunc", "noerror", "fdatasync", "fsync",
        ],
        "iflag" | "oflag" => [
            "append", "direct", "directory", "dsync", "sync", "fullblock", "nonblock", "noatime", "nocache",
            "noctty", "nofollow", "count_bytes", "skip_bytes", "seek_bytes",
        ],
        "status" => ["none", "noxfer", "progress"],
        _ => [],
    };
    // A list: the symbol after the last comma.
    let comma = rest.index_of(",");
    let start = eq + 1;
    while comma >= 0 {
        start += comma + 1;
        rest = rest.sub_string(comma + 1);
        comma = rest.index_of(",");
    }
    #{prefix: cur.sub_string(0, start), candidates: values}
}

// A TOML key or string without its quotes.
fn unquote(s) {
    s.trim();
    if s.len() >= 2 && (s.starts_with("\"") && s.ends_with("\"") || s.starts_with("'") && s.ends_with("'")) {
        s.sub_string(1, s.len() - 2)
    } else {
        s
    }
}

// The strings of a TOML array (`["a", "b"]`).
fn toml_list(v) {
    v.trim();
    if !v.starts_with("[") {
        return [];
    }
    v = v.sub_string(1, v.len() - 2);
    v.split(",").map(|x| unquote(x)).filter(|x| x != "")
}

// The tables of a TOML file, enough for the names in manifests
// (Cargo.toml, pixi.toml ...): a map from each table's name (`package`,
// `target.linux-64.tasks`) to a map from its keys to their values, as
// written. An array of tables (`[[bin]]`) is an array of such maps.
fn toml(text) {
    let out = #{};
    let name = "";
    let many = false;
    let key = ();
    let val = "";
    out[""] = #{};
    for line in text.split("\n") {
        let l = line;
        l.trim();
        if key != () {
            val += " " + l;
            if val.split("[").len() <= val.split("]").len() {
                if many { out[name][-1][key] = val; } else { out[name][key] = val; }
                key = ();
            }
            continue;
        }
        if l == "" || l.starts_with("#") {
            continue;
        }
        if l.starts_with("[[") {
            name = unquote(l.sub_string(2, l.index_of("]]") - 2));
            many = true;
            if out[name] == () {
                out[name] = [];
            }
            out[name].push(#{});
        } else if l.starts_with("[") {
            name = l.sub_string(1, l.index_of("]") - 1);
            name.trim();
            many = false;
            if out[name] == () {
                out[name] = #{};
            }
        } else {
            let eq = l.index_of("=");
            if eq <= 0 {
                continue;
            }
            let k = unquote(l.sub_string(0, eq));
            let v = l.sub_string(eq + 1);
            v.trim();
            if v.starts_with("[") && v.split("[").len() > v.split("]").len() {
                key = k;
                val = v;
            } else if many {
                out[name][-1][k] = v;
            } else {
                out[name][k] = v;
            }
        }
    }
    out
}

// The commands in `PATH` that start with `cur` (the executables, as the
// command line completes them), or () for a path.
fn commands(cur) {
    if cur.contains("/") { () } else { sh::commands(cur) }
}

fn complete(kind, cur, words) {
    if type_of(kind) == "array" {
        return kind;
    }
    let colon = kind.index_of(":");
    if colon > 0 {
        let m = kind.sub_string(0, colon);
        import m as specs;
        return specs::kind(kind.sub_string(colon + 1), cur, words);
    }
    switch kind {
        "files" => (),
        "none" => [],
        "dirs" => dirs(cur),
        "users" => users(),
        "groups" => groups(),
        "owner" => owner(cur),
        "mode" => mode(),
        "signals" => signals(),
        "processes" => processes(true),
        "pids" => processes(false),
        "hosts" => hosts(cur, " "),
        "remote" => remote(cur),
        "manpages" => man_pages(cur, man_section(words)),
        "sections" => sections(),
        "targets" => targets(words),
        "members" => members(cur, words),
        "dd" => dd(cur),
        "commands" => commands(cur),
        _ => throw `unknown kind: ${kind}`,
    }
}
