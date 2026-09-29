# sh::builtin registers a command. It gets its words as an array (the name
# first) and returns its status: () is 0, a boolean true 0 and false 1, an
# integer modulo 256. A string thrown is reported as the command's error,
# other errors with the extension's file; either way the status is 1.
cat > b.rhai <<'P'
sh::builtin("greet", |argv| {
    if argv.len() < 2 { throw "usage: greet NAME..."; }
    for name in argv.extract(1) { print(`hello, ${name}`); }
});
fn show(argv) {
    print(`${argv.len()} words: ${argv}, X=${sh::getvar("X") ?? "unset"}`);
}
sh::builtin("show", show);
sh::builtin("is_odd", |argv| parse_int(argv[1]) % 2 == 1);
sh::builtin("status", |argv| parse_int(argv[1]));
sh::builtin("broken", |argv| no_such_function());
sh::builtin("wrong", |argv| "a string");
sh::builtin("readonly_set", |argv| sh::setvar("R", "x"));
sh::builtin("lines", |argv| {
    let n = 0;
    loop {
        let line = sh::read_line();
        if line == () { break; }
        n += 1;
        print(`${n}: ${line}`);
        if argv.len() > 1 && n == parse_int(argv[1]) { break; }
    }
});
sh::builtin("leave", |argv| sh::run("exit 7"));
sh::builtin("countdown", |argv| {
    let n = parse_int(argv[1]);
    print(n);
    if n > 0 { sh::run(`countdown ${n - 1}`) } else { 0 }
});
for name in ["cd", "plugin", "a/b", ""] {
    try { sh::builtin(name, |argv| 0); } catch (e) { print(e); }
}
P
__luish_internal plugin load ./b.rhai
greet world "two words"; echo "greet: $?"
greet; echo "no args: $?"
X=temporary show 'a b' ''; echo "X=${X-unset}"
is_odd 3; echo "3 odd: $?"; is_odd 4; echo "4 odd: $?"
status 0; echo "0: $?"; status 300; echo "300: $?"; status -1; echo "-1: $?"
broken; echo "broken: $?"
wrong; echo "wrong: $?"
readonly R=1
readonly_set; echo "readonly: $?"
# Commands, redirections, pipelines and substitutions as for any built-in.
type greet; command -V greet; command -v greet
command greet command; builtin greet builtin
greet redirected > out.txt; cat out.txt
greet pipe | tr a-z A-Z
out=$(greet substitution); echo "[$out]"
countdown 3
# sh::read_line reads from fd 0 a byte at a time, leaving the rest.
printf 'one\ntwo\nthree' | lines; echo "lines: $?"
{ lines 1; cat; } <<'H'
first
rest 1
rest 2
H
lines < /dev/null; echo "empty: $?"
# A shell function comes first, as for other regular built-ins.
greet() { echo "the function"; }
greet; builtin greet via-builtin; command greet via-command
unset -f greet
(leave; echo not reached); echo "leave: $?"
# Unloading (or loading again) removes them.
__luish_internal plugin unload b
greet 2>/dev/null; echo "unloaded: $?"
type greet; echo "type: $?"
