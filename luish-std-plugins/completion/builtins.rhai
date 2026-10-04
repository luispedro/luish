// Specs for luish's own built-ins (see lib.rhai for their fields), and the
// completers of `style` and `__luish_internal`.
//
// luish completes the arguments of many built-ins itself (directories for
// `cd`, variables for `unset`, jobs for `fg`, aliases for `unalias` ...),
// which only it knows; a completer that returns () gets those. So these
// specs give the options, and leave the other arguments to luish, with the
// kind `files` (which is ()).

// The variables' attributes, which typeset, declare and local share.
fn attributes() {
    `
        -a  make the variables arrays
        -A  make the variables associative arrays
        -i  give the variables the integer attribute
        -l  convert the values assigned to lower case
        -r  make the variables read-only
        -u  convert the values assigned to upper case
        -U  keep only the first of equal elements in arrays
        -x  export the variables
    `
}

fn typeset() {
    #{
        opts: attributes() + `
            -f  print the definitions of the functions
            -g  in a function, change the variables outside it
            -p  print the variables, with their attributes
        `,
    }
}

fn print_spec() {
    #{
        opts: `
            -a  with -c or -C, fill the rows across
            -b  recognise bindkey's escapes
            -c  write the arguments in columns
            -C COLS  write the arguments in COLS columns
            -D  replace $HOME at the start of an argument with ~
            -f FORMAT  format the arguments as printf does
            -i  sort without regard to case (with -o or -O)
            -l  separate the arguments by newlines
            -m PATTERN  keep only the arguments that match PATTERN
            -n  don't write the newline at the end
            -N  separate and end the arguments with NUL bytes
            -o  sort the arguments in ascending order
            -O  sort the arguments in descending order
            -P  expand % sequences as in prompts
            -r  don't expand backslash escapes
            -R  emulate BSD echo
            -s  add the arguments to the history
            -u FD  write to file descriptor FD
            -v NAME  set the variable NAME to the output
            -x TAB-STOP  expand the tabs at the start of each line
            -X TAB-STOP  expand all tabs
            -z  push the arguments onto the editing buffer stack
        `,
        values: #{"-C": "none", "-f": "none", "-m": "none", "-u": "none", "-v": "none", "-x": "none", "-X": "none"},
    }
}

fn ulimit() {
    #{
        opts: `
            -H  only the hard limit
            -S  only the soft limit
            -a  show all the limits
            -t  CPU time, in seconds
            -f  file size, in blocks of 512 bytes
            -d  data segment, in KiB
            -s  stack, in KiB
            -c  core file size, in blocks of 512 bytes
            -m  resident memory, in KiB
            -l  locked memory, in KiB
            -p  processes
            -n  open files
            -v  address space, in KiB
            -w  file locks
            -r  real-time priority
        `,
        args: [["unlimited"]],
    }
}

// The tests of `test` and `[`, as options (with `single_dash`, so that
// `-nt` isn't read as `-n -t`). The binary ones are offered too, after
// an operand.
fn test_spec() {
    #{
        opts: `
            -b FILE  FILE is a block device
            -c FILE  FILE is a character device
            -d FILE  FILE is a directory
            -e FILE  FILE exists
            -f FILE  FILE is a regular file
            -g FILE  FILE is set-group-ID
            -h FILE  FILE is a symbolic link
            -k FILE  FILE has its sticky bit set
            -L FILE  FILE is a symbolic link
            -p FILE  FILE is a FIFO
            -r FILE  FILE is readable
            -s FILE  FILE is not empty
            -S FILE  FILE is a socket
            -t FD  FD is a terminal
            -u FILE  FILE is set-user-ID
            -w FILE  FILE is writable
            -x FILE  FILE is executable
            -O FILE  FILE is owned by the effective user
            -G FILE  FILE is owned by the effective group
            -n STRING  STRING is not empty
            -z STRING  STRING is empty
            -eq  equal (numbers)
            -ne  not equal (numbers)
            -lt  less than (numbers)
            -le  less than or equal (numbers)
            -gt  greater than (numbers)
            -ge  greater than or equal (numbers)
            -nt  newer than (files)
            -ot  older than (files)
            -ef  the same file
            -a  and
            -o  or
        `,
        values: #{"-t": "none", "-n": "none", "-z": "none"},
        single_dash: true,
    }
}

fn spec(cmd) {
    switch cmd {
        "alias" => #{
            opts: `
                -g  global aliases, expanded anywhere
                -s  suffix aliases, for files that end in .NAME
                -r  regular aliases (for listing)
                -m  take the names as patterns
                -L  list the aliases as alias commands
            `,
        },
        "unalias" => #{
            opts: `
                -a  remove all regular and global aliases
                -m  take the names as patterns
                -s  suffix aliases
            `,
        },
        "bindkey" => #{
            opts: `
                -L  list the bindings as bindkey commands
                -r  remove the bindings of the keys
                -e  use emacs key bindings
                -v  use vi key bindings
            `,
        },
        "cd" | "chdir" => #{
            opts: `
                -L  follow the path as given (logical)
                -P  resolve symbolic links (physical)
                -e  with -P, fail if the new directory's name can't be found
            `,
        },
        "pushd" => #{
            opts: `
                -q  don't print the stack
                -L  follow the path as given (logical)
                -P  resolve symbolic links (physical)
            `,
        },
        "popd" => #{
            opts: `
                -q  don't print the stack
                -L  follow the path as given (logical)
                -P  resolve symbolic links (physical)
            `,
            args: ["none"],
        },
        "dirs" => #{
            opts: `
                -c  clear the stack
                -l  print full names, without ~
                -p  print one directory per line
                -v  print one directory per line, with its number
            `,
            args: ["dirs"],
        },
        "echo" => #{opts: "-n  don't write the newline at the end"},
        "fc" => #{
            opts: `
                -e EDITOR  edit with EDITOR (- runs without editing)
                -l  list the commands
                -n  with -l, leave out the event numbers
                -r  reverse the order of the commands
                -s  run a command again without editing it
            `,
            values: #{"-e": "commands"},
            args: ["none"],
        },
        "hash" => #{opts: "-r  forget all remembered paths"},
        "jobs" => #{
            opts: `
                -l  also show the process IDs
                -p  show only the process group IDs
            `,
        },
        "pwd" => #{
            opts: `
                -L  print the logical directory
                -P  print the physical directory
            `,
            args: ["none"],
        },
        "read" => #{
            opts: `
                -r  treat backslashes as ordinary characters
                -p PROMPT  print PROMPT first
                -A  read the fields into an array
                -a  read the fields into an array
            `,
            values: #{"-p": "none"},
        },
        "umask" => #{opts: "-S  show the mask in symbolic form", args: ["none"]},
        "unset" => #{
            opts: `
                -v  remove variables
                -f  remove functions
            `,
        },
        "export" | "readonly" => #{opts: "-p  print the variables"},
        "local" => #{opts: attributes()},
        "typeset" | "declare" => typeset(),
        "print" => print_spec(),
        "ulimit" => ulimit(),
        "test" | "[" => test_spec(),
        "set" => {
            import "shells" as shells;
            #{opts: shells::set_letters() + "\n-o NAME  set an option by name", values: #{"-o": "shells:luish"}}
        },
        _ => throw `no spec for ${cmd}`,
    }
}

// `style`.

// Each line of `__luish_internal style ARGS...`, as [its first word, the
// rest].
fn style_lines(args) {
    let out = [];
    let r = sh::capture_sh("__luish_internal style " + sh::quote(args) + " 2>/dev/null");
    for line in r.out.split("\n") {
        line.trim();
        if line == "" {
            continue;
        }
        let gap = line.index_of(" ");
        if gap < 0 {
            out.push([line, ""]);
        } else {
            let rest = line.sub_string(gap);
            rest.trim();
            out.push([line.sub_string(0, gap), rest]);
        }
    }
    out
}

// The names of styles, with their values as descriptions (or, in the
// scheme `scheme` if it isn't (), the values it gives them).
fn style_names(scheme) {
    let lines = style_lines([]);
    if scheme == () {
        return lines.map(|l| #{value: l[0], desc: l[1]});
    }
    let own = #{};
    for l in style_lines(["-s", scheme]) {
        own[l[0]] = l[1];
    }
    let names = lines.map(|l| l[0]);
    for k in ["terminal.background", "terminal.foreground", "terminal.cursor", "terminal.palette"] {
        names.push(k);
    }
    names.map(|n| if own[n] != () {
        #{value: n, desc: own[n]}
    } else if n.starts_with("terminal.") {
        #{value: n, desc: "the terminal's colour"}
    } else {
        n
    })
}

// The colour schemes, with the one in use described so.
fn schemes() {
    let out = [];
    for l in style_lines(["-c"]) {
        if l[0] == "*" {
            let rest = l[1];
            let gap = rest.index_of(" ");
            out.push(#{value: if gap < 0 { rest } else { rest.sub_string(0, gap) }, desc: "in use"});
        } else {
            out.push(l[0]);
        }
    }
    out
}

fn colours() {
    let names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    ["default"] + names + names.map(|n| "bright-" + n)
}

// A word of a style's value (which may be several words in one argument,
// as in 'italic bright-black').
fn style_value(cur) {
    let prefix = "";
    let space = cur.index_of(" ");
    while space >= 0 {
        prefix = cur.sub_string(0, space + 1);
        let next = cur.sub_string(space + 1).index_of(" ");
        space = if next < 0 { -1 } else { space + 1 + next };
    }
    let word = cur.sub_string(prefix.len());
    for p in ["bg:", "ul:"] {
        if word.starts_with(p) {
            return #{prefix: prefix + p, candidates: colours()};
        }
    }
    let attrs = ["bold", "dim", "italic", "underline", "blink", "reverse", "strike"];
    let out = colours() + attrs + attrs.map(|a| "no-" + a);
    out += ["undercurl", "underdouble", "underdotted", "underdashed", "plain"];
    out += [
        #{value: "bg:", desc: "background colour", suffix: ""},
        #{value: "ul:", desc: "underline colour", suffix: ""},
        #{value: "sgr:", desc: "raw SGR parameters", suffix: ""},
    ];
    if prefix == "" { out } else { #{prefix: prefix, candidates: out} }
}

fn style_options() {
    [
        #{value: "-p", desc: "print commands that recreate the styles"},
        #{value: "-r", desc: "remove the styles of the names"},
        #{value: "--clear", desc: "remove all styles, and choose no scheme"},
        #{value: "-c", desc: "choose a colour scheme (or list them)"},
        #{value: "--detect", desc: "ask the terminal for its background again"},
        #{value: "--terminal-colors", desc: "whether schemes set the terminal's colours"},
        #{value: "-s", desc: "define or change a colour scheme"},
        #{value: "-d", desc: "set or remove a default, under the scheme"},
    ]
}

// After `NAME` or `-r` (`scheme`: in that scheme, or ()): `args` are the
// arguments before the word `cur`.
fn style_set(args, cur, scheme) {
    if args.is_empty() || args[0] == "-r" {
        return style_names(scheme);
    }
    if args[0].starts_with("terminal.") {
        return [];   // #rrggbb
    }
    style_value(cur)
}

// `style ARGS... CUR` (`words[0]` is `style`; for `__luish_internal style`,
// the words from `style`).
fn style(words, i) {
    let args = words.extract(1, i - 1);
    let cur = words[i];
    if args.is_empty() {
        return if cur.starts_with("-") { style_options() } else { style_names(()) };
    }
    let rest = args.extract(1);
    switch args[0] {
        "-c" => if rest.len() < 3 { schemes() } else { [] },
        "--terminal-colors" | "--terminal-colours" => if rest.is_empty() { ["on", "off"] } else { [] },
        "-r" => style_names(()),
        "-p" | "--clear" | "--detect" => [],
        "-d" => {
            if rest.is_empty() && cur.starts_with("-") {
                return [#{value: "-r", desc: "remove the defaults of the names"}];
            }
            style_set(rest, cur, ())
        }
        "-s" => {
            if rest.is_empty() {
                return schemes();
            }
            let scheme = rest[0];
            let rest = rest.extract(1);
            if rest.is_empty() && cur.starts_with("-") {
                return [
                    #{value: "-r", desc: "remove styles from the scheme"},
                    #{value: "-i", desc: "set the scheme it inherits from"},
                    #{value: "--delete", desc: "remove the scheme"},
                ];
            }
            if rest.is_empty() {
                return style_names(scheme);
            }
            switch rest[0] {
                "-i" => if rest.len() == 1 { schemes() } else { [] },
                "--delete" => [],
                _ => style_set(rest, cur, scheme),
            }
        }
        _ => style_set(args, cur, ()),
    }
}

// `__luish_internal`.

fn internal_commands() {
    `
        bindkey  show or change the key bindings
        check-cache  check the startup caches
        clipcopy  put text on the clipboard
        complete  print what Tab offers for a line
        function-file  set where a function was defined
        help  show the help of built-ins
        plugin  load and manage plugins
        print  print arguments
        print-git-rev  print the current git revision
        print-git-rev-short  print the current git revision, short
        savestate  save the shell's state
        style  show or change the styles
        where  show where variables were set
    `
}

fn internal(words, i) {
    import "lib" as lib;
    if i == 1 {
        return lib::parse_commands(internal_commands()).map(|c| #{value: c.names[0], desc: c.desc});
    }
    let sub = words.extract(1);
    switch words[1] {
        "style" => style(sub, i - 1),
        "bindkey" | "print" => lib::complete(spec(words[1]), sub, i - 1),
        "check-cache" => lib::complete(
            #{opts: "-q, --quiet  show only the caches that were rebuilt", args: [["rc", "login", "startup"]]},
            sub,
            i - 1,
        ),
        "complete" | "print-git-rev" | "print-git-rev-short" | "savestate" => [],
        _ => (),
    }
}
