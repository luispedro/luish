// Completion from bash-completion, for the commands that have no completer
// of their own: bridge.bash runs the command's bash completion and prints
// what it gives.

let bridge = sh::plugin_dir() + "/bridge.bash";

sh::completer("-default-", |words, i| {
    let r = sh::capture(["env", "BASH_ENV=", "bash", bridge, `${i}`] + words);
    if r.status != 0 {
        return ();   // filenames
    }
    let lines = r.out.split("\n");
    let prefix = lines.shift();
    let out = [];
    for line in lines {
        if line.ends_with("\t") {
            out.push(#{value: line.sub_string(0, line.len() - 1), suffix: ""});
        } else if line != "" {
            out.push(line);
        }
    }
    #{prefix: prefix, candidates: out}
});
