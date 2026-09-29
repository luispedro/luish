// The benchmark's commands as an extension's built-ins, with the same
// output as the shell functions in tasks.lsh.

sh::builtin("collatz_max", |argv| {
    let n = parse_int(argv[1]);
    let best = 0;
    let best_len = 0;
    for i in 1..n {
        let x = i;
        let len = 1;
        while x != 1 {
            x = if x % 2 == 0 { x / 2 } else { 3 * x + 1 };
            len += 1;
        }
        if len > best_len {
            best = i;
            best_len = len;
        }
    }
    print(`${best} ${best_len}`);
});

sh::builtin("wordfreq", |argv| {
    let text = fs::read_file(argv[1]);
    if text == () { throw `${argv[1]}: cannot open`; }
    let count = #{};
    let seen = [];
    let total = 0;
    for line in text.split("\n") {
        for w in line.split(" ") {
            if w == "" { continue; }
            let n = count[w];
            if n == () {
                seen.push(w);
                count[w] = 1;
            } else {
                count[w] = n + 1;
            }
            total += 1;
        }
    }
    print(`words: ${total}, distinct: ${seen.len()}`);
    for k in 1..=5 {
        let best = "";
        let best_n = 0;
        for w in seen {
            if count[w] > best_n {
                best = w;
                best_n = count[w];
            }
        }
        print(`${k}. ${best} ${best_n}`);
        count[best] = 0;
    }
});

sh::builtin("urlencode", |argv| {
    let out = "";
    for c in argv[1].chars() {
        if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || "._~-".contains(c) {
            out += c;
        } else {
            let h = c.to_int().to_hex().to_upper();
            out += if h.len() < 2 { `%0${h}` } else { `%${h}` };
        }
    }
    sh::setvar("REPLY", out);
});

sh::builtin("add", |argv| sh::setvar("REPLY", `${parse_int(argv[1]) + parse_int(argv[2])}`));
