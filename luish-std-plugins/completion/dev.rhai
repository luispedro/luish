// Specs for development tools: cargo and rustup (from their own `-h`), go,
// C compilers (gcc, clang), cmake, meson, ninja, gdb, python, sqlite3 and
// editors (vim, nvim, nano, emacs) (see lib.rhai for their fields).

import "kinds" as kinds;
import "lib" as lib;

// The spec of a program from its `-h`, whose subcommands are also read
// from theirs (`path` is the command, with its subcommands).
fn from_help(path) {
    let s = lib::help_spec(lib::help_of(path));
    s.guess_values = true;
    s.sub_spec = "dev:" + path;
    s
}

// The nearest Cargo.toml, read, or ().
fn manifest() {
    let p = fs::find_up("Cargo.toml");
    if p == () { () } else { kinds::toml(fs::read_file(p) ?? "") }
}

// The directory of the nearest Cargo.toml.
fn manifest_dir() {
    let p = fs::find_up("Cargo.toml");
    if p == () { () } else { kinds::split_dir(p)[0] }
}

// The packages of the workspace: the package of the nearest Cargo.toml,
// and the members of its workspace.
fn cargo_packages() {
    let m = manifest();
    if m == () {
        return [];
    }
    let out = [];
    let name = (m["package"] ?? #{})["name"];
    if name != () {
        out.push(kinds::unquote(name));
    }
    let dir = manifest_dir();
    for member in kinds::toml_list((m["workspace"] ?? #{})["members"] ?? "") {
        let dirs = [member];
        if member.ends_with("/*") {
            let base = member.sub_string(0, member.len() - 1);
            dirs = (fs::list_dir(dir + base) ?? []).map(|d| base + d);
        }
        for d in dirs {
            let t = kinds::toml(fs::read_file(dir + d + "/Cargo.toml") ?? "");
            let n = (t["package"] ?? #{})["name"];
            if n != () {
                out.push(kinds::unquote(n));
            }
        }
    }
    out
}

// The targets of a kind (`bin`, `example`, `test`, `bench`) of the package:
// the files of their directory, and those named in Cargo.toml.
fn cargo_targets(kind) {
    let m = manifest();
    if m == () {
        return [];
    }
    let dir = manifest_dir();
    let sub = switch kind {
        "bin" => "src/bin",
        "example" => "examples",
        "test" => "tests",
        "bench" => "benches",
    };
    let out = [];
    for f in fs::list_dir(dir + sub) ?? [] {
        if f.ends_with(".rs") {
            out.push(f.sub_string(0, f.len() - 3));
        } else if fs::is_file(dir + sub + "/" + f + "/main.rs") {
            out.push(f);
        }
    }
    if kind == "bin" && fs::is_file(dir + "src/main.rs") {
        let name = (m["package"] ?? #{})["name"];
        if name != () {
            out.push(kinds::unquote(name));
        }
    }
    for t in m[kind] ?? [] {
        if t["name"] != () && !(kinds::unquote(t["name"]) in out) {
            out.push(kinds::unquote(t["name"]));
        }
    }
    out
}

// The features of the package, after the last comma.
fn cargo_features(cur) {
    let m = manifest();
    let feats = if m == () { [] } else { (m["features"] ?? #{}).keys() };
    let comma = -1;
    let k = 0;
    for c in cur.chars() {
        if c == ',' || c == ' ' {
            comma = k;
        }
        k += 1;
    }
    #{prefix: cur.sub_string(0, comma + 1), candidates: feats}
}

// The dependencies of the package.
fn cargo_deps() {
    let m = manifest();
    if m == () {
        return [];
    }
    let out = [];
    for t in ["dependencies", "dev-dependencies", "build-dependencies"] {
        for d in (m[t] ?? #{}).keys() {
            if !(d in out) {
                out.push(d);
            }
        }
    }
    out
}

// The profiles of the package.
fn cargo_profiles() {
    let out = ["dev", "release", "test", "bench"];
    let m = manifest();
    if m != () {
        for t in m.keys() {
            if t.starts_with("profile.") && !(t.sub_string(8) in out) {
                out.push(t.sub_string(8));
            }
        }
    }
    out
}

// The installed toolchains of rustup.
fn toolchains() {
    let r = sh::capture("rustup toolchain list 2>/dev/null");
    let out = ["stable", "beta", "nightly"];
    for line in r.out.split("\n") {
        let t = line.split();
        if !t.is_empty() && !(t[0] in out) {
            out.push(t[0]);
        }
    }
    out
}

fn rust_targets() {
    let r = sh::capture("rustc --print target-list 2>/dev/null");
    r.out.split("\n").filter(|t| t != "")
}

fn rust_components() {
    let r = sh::capture("rustup component list 2>/dev/null");
    let out = [];
    for line in r.out.split("\n") {
        let w = line.split();
        if !w.is_empty() {
            let desc = w.extract(1).reduce(|a, b| a + " " + b, "");
            desc.trim();
            out.push(#{value: w[0], desc: desc});
        }
    }
    out
}

// The targets of the build.ninja in the directory of `-C`.
fn ninja_targets(words) {
    let dir = kinds::option_value(words, "-C", "");
    let file = kinds::option_value(words, "-f", "");
    let cmd = "ninja" + (if dir == () { "" } else { " -C " + sh::quote(kinds::tilde(dir)) })
        + (if file == () { "" } else { " -f " + sh::quote(kinds::tilde(file)) }) + " -t targets all 2>/dev/null";
    let out = [];
    for line in sh::capture(cmd).out.split("\n") {
        let c = line.index_of(": ");
        if c > 0 {
            out.push(line.sub_string(0, c));
        }
    }
    out
}

// The tags of the tags file of the current directory.
fn tags() {
    let out = [];
    let seen = #{};
    for line in (fs::read_file("tags") ?? "").split("\n") {
        if line.starts_with("!") {
            continue;
        }
        let tab = line.index_of("\t");
        if tab > 0 {
            let t = line.sub_string(0, tab);
            if !seen.contains(t) {
                seen[t] = true;
                out.push(t);
            }
        }
    }
    out
}

// Python modules that are often run with `-m`, and those of the current
// directory.
fn python_modules() {
    let out = [
        "pip", "venv", "http.server", "json.tool", "pdb", "timeit", "unittest", "cProfile", "profile", "pydoc",
        "ensurepip", "zipfile", "tarfile", "site", "doctest", "trace", "calendar", "compileall", "py_compile",
        "pytest", "mypy", "black", "ruff", "IPython", "jupyter", "build", "twine", "pipx", "uvicorn", "flask",
    ];
    for f in fs::list_dir(".") ?? [] {
        if f.ends_with(".py") {
            out.push(f.sub_string(0, f.len() - 3));
        } else if fs::is_file(f + "/__init__.py") || fs::is_file(f + "/__main__.py") {
            out.push(f);
        }
    }
    out
}

fn kind(name, cur, words) {
    switch name {
        "cargo_packages" => cargo_packages(),
        "cargo_bins" => cargo_targets("bin"),
        "cargo_examples" => cargo_targets("example"),
        "cargo_tests" => cargo_targets("test"),
        "cargo_benches" => cargo_targets("bench"),
        "cargo_features" => cargo_features(cur),
        "cargo_deps" => cargo_deps(),
        "cargo_profiles" => cargo_profiles(),
        "toolchains" => toolchains(),
        "rust_targets" => rust_targets(),
        "rust_components" => rust_components(),
        "ninja_targets" => ninja_targets(words),
        "tags" => tags(),
        "python_modules" => python_modules(),
        _ => throw `unknown kind: ${name}`,
    }
}

// The values of cargo's options that its `-h` doesn't give.
fn cargo_values() {
    #{
        "-p": "dev:cargo_packages", "--package": "dev:cargo_packages", "--exclude": "dev:cargo_packages",
        "--bin": "dev:cargo_bins", "--example": "dev:cargo_examples", "--test": "dev:cargo_tests",
        "--bench": "dev:cargo_benches", "-F": "dev:cargo_features", "--features": "dev:cargo_features",
        "--profile": "dev:cargo_profiles", "--target": "dev:rust_targets", "--manifest-path": "files",
        "--target-dir": "dirs", "--artifact-dir": "dirs", "-C": "dirs",
        "--message-format": [
            "human", "short", "json", "json-diagnostic-short", "json-diagnostic-rendered-ansi",
            "json-render-diagnostics",
        ],
        "--timings": ["html", "json"],
    }
}

// cargo, with its commands from `cargo --list` (with the aliases and the
// commands of plugins). Its own options are here, to run cargo once less.
fn cargo() {
    let s = #{
        opts: `
            -V, --version  print version info and exit
            --list  list installed commands
            --explain <CODE>  provide a detailed explanation of a rustc error message
            -v, --verbose  use verbose output (-vv very verbose/build.rs output)
            -q, --quiet  do not print cargo log messages
            --color <WHEN>  coloring
            -C <DIRECTORY>  change to DIRECTORY before doing anything (nightly-only)
            --locked  assert that Cargo.lock will remain unchanged
            --offline  run without accessing the network
            --frozen  equivalent to specifying both --locked and --offline
            --config <KEY=VALUE|PATH>  override a configuration value
            -Z <FLAG>  unstable (nightly-only) flags to Cargo
            -h, --help  print help
        `,
        values: #{"--color": ["auto", "always", "never"], "--explain": "none", "--config": "none", "-Z": "none"},
    };
    let names = #{};
    let order = [];
    let descs = #{};
    for line in sh::capture("cargo --list 2>/dev/null").out.split("\n") {
        if !line.starts_with(" ") {
            continue;
        }
        let w = line.split();
        if w.is_empty() {
            continue;
        }
        let desc = w.extract(1).reduce(|a, b| a + " " + b, "");
        desc.trim();
        if desc.starts_with("alias: ") {
            let target = desc.sub_string(7).split()[0];
            names[target] = (names[target] ?? [target]) + [w[0]];
            if !(target in order) {
                order.push(target);
            }
        } else {
            if names[w[0]] == () {
                names[w[0]] = [w[0]];
            }
            if !(w[0] in order) {
                order.push(w[0]);
            }
            if desc.len() > 1 && desc[1] >= 'a' && desc[1] <= 'z' {
                desc = desc[0].to_lower() + desc.sub_string(1);
            }
            descs[w[0]] = desc;
        }
    }
    let commands = "";
    for c in order {
        commands += names[c].reduce(|a, b| if a == "" { b } else { a + ", " + b }, "") + "  " + (descs[c] ?? "") + "\n";
    }
    if commands != "" {
        s.commands = commands;
    }
    s.values.mixin(cargo_values());
    s.guess_values = true;
    s.sub_spec = "dev:cargo";
    s.skip = #{"+": "dev:toolchains"};
    s
}

fn sub_spec(path, sub) {
    let s = from_help(path + " " + sub);
    let v = if path.starts_with("cargo") { cargo_values() } else { #{} };
    if path == "cargo" {
        s.args = switch sub {
            "remove" | "rm" => ["dev:cargo_deps"],
            "add" | "install" | "search" | "uninstall" | "yank" | "owner" | "info" | "test" | "t" | "bench" => ["none"],
            "new" | "init" => ["dirs"],
            "help" => [parse_commands_of("cargo")],
            _ => s.args,
        };
    } else if path == "rustup" {
        s.args = switch sub {
            "default" => ["dev:toolchains"],
            "run" => ["dev:toolchains", "commands", "files"],
            "doc" | "man" => ["none"],
            _ => s.args,
        };
    } else if path == "rustup toolchain" && (sub == "uninstall" || sub == "link") {
        s.args = ["dev:toolchains", "dirs"];
    } else if path == "rustup target" && (sub == "add" || sub == "remove") {
        s.args = ["dev:rust_targets"];
    } else if path == "rustup component" && (sub == "add" || sub == "remove") {
        s.args = ["dev:rust_components"];
    } else if path == "rustup override" && sub == "set" {
        s.args = ["dev:toolchains"];
    }
    v.mixin(s.values);
    if path.starts_with("rustup") {
        v["--toolchain"] = "dev:toolchains";
        v["--target"] = "dev:rust_targets";
    }
    s.values = v;
    s
}

// The names of the subcommands of a program, from its `-h`.
fn parse_commands_of(path) {
    let s = lib::help_spec(lib::help_of(path));
    lib::parse_commands(s.commands ?? "").map(|c| #{value: c.names[0], desc: c.desc})
}

fn rustup() {
    let s = from_help("rustup");
    s.values["--toolchain"] = "dev:toolchains";
    s.skip = #{"+": "dev:toolchains"};
    s
}

fn golang() {
    let build = `
        -C DIR  change to DIR before running the command
        -a  force rebuilding of packages that are already up-to-date
        -n  print the commands but do not run them
        -p N  the number of programs to run in parallel
        -race  enable data race detection
        -msan  enable interoperation with memory sanitizer
        -asan  enable interoperation with address sanitizer
        -cover  enable code coverage instrumentation
        -v  print the names of packages as they are compiled
        -work  print the name of the temporary work directory and don't delete it
        -x  print the commands
        -o FILE  write the resulting executable or object to FILE
        -buildmode MODE  the build mode to use
        -buildvcs MODE  whether to stamp binaries with version control information
        -gcflags FLAGS  arguments to pass on each go tool compile invocation
        -ldflags FLAGS  arguments to pass on each go tool link invocation
        -mod MODE  the module download mode (readonly, vendor, mod)
        -modfile FILE  read and write an alternate go.mod file
        -overlay FILE  read a JSON config file that provides an overlay for build operations
        -pgo FILE  the file path of a profile for profile-guided optimization
        -tags TAGS  a comma-separated list of additional build tags
        -trimpath  remove all file system paths from the resulting executable
        -json  emit build output in JSON
    `;
    let pkg = #{opts: build, single_dash: true, guess_values: true, values: #{
        "-buildmode": ["archive", "c-archive", "c-shared", "default", "shared", "exe", "pie", "plugin"],
        "-mod": ["readonly", "vendor", "mod"], "-buildvcs": ["true", "false", "auto"],
    }};
    let test = pkg;
    test.opts = build + `
        -run REGEXP  run only the tests matching REGEXP
        -bench REGEXP  run the benchmarks matching REGEXP
        -benchmem  print memory allocation statistics for benchmarks
        -benchtime T  run each benchmark for T
        -count N  run each test and benchmark N times
        -coverprofile FILE  write a coverage profile to FILE
        -cpu LIST  a list of GOMAXPROCS values to run the tests with
        -failfast  do not start new tests after the first failure
        -fuzz REGEXP  run the fuzz test matching REGEXP
        -list REGEXP  list the tests, benchmarks, fuzz tests or examples matching REGEXP
        -parallel N  allow parallel execution of N tests
        -short  tell long-running tests to shorten their run time
        -skip REGEXP  skip the tests matching REGEXP
        -timeout D  panic if a test binary runs longer than D
        -shuffle MODE  randomize the order of tests (off, on, N)
    `;
    #{
        commands: `
            bug  start a bug report
            build  compile packages and dependencies
            clean  remove object files and cached files
            doc  show documentation for a package or symbol
            env  print Go environment information
            fix  update packages to use new APIs
            fmt  gofmt (reformat) package sources
            generate  generate Go files by processing source
            get  add dependencies to the current module and install them
            install  compile and install packages and dependencies
            list  list packages or modules
            mod  module maintenance
            work  workspace maintenance
            run  compile and run a Go program
            telemetry  manage telemetry data and settings
            test  test packages
            tool  run a specified go tool
            version  print the Go version
            vet  report likely mistakes in packages
            help  show the help of a command or topic
        `,
        subs: #{
            build: pkg, install: pkg, run: pkg, test: test, vet: pkg, list: pkg, get: pkg,
            env: #{opts: "-json  print in JSON\n-u  unset the variables\n-w  set the variables", single_dash: true,
                args: [["GOARCH", "GOBIN", "GOCACHE", "GOFLAGS", "GOMODCACHE", "GOOS", "GOPATH", "GOPROXY",
                    "GOROOT", "GOPRIVATE", "GOTOOLCHAIN", "CGO_ENABLED"]]},
            mod: #{commands: `
                download  download modules to the local cache
                edit  edit go.mod from tools or scripts
                graph  print the module requirement graph
                init  initialize a new module in the current directory
                tidy  add missing and remove unused modules
                vendor  make a vendored copy of the dependencies
                verify  verify that the dependencies have the expected content
                why  explain why packages or modules are needed
            `},
            work: #{commands: `
                edit  edit go.work from tools or scripts
                init  initialize a workspace file
                sync  sync the workspace build list back to the modules
                use  add modules to the workspace file
                vendor  make a vendored copy of the dependencies
            `},
            help: #{args: [["build", "clean", "doc", "env", "fix", "fmt", "generate", "get", "install", "list",
                "mod", "work", "run", "test", "tool", "version", "vet", "buildconstraint", "buildmode", "c",
                "cache", "environment", "filetype", "go.mod", "gopath", "goproxy", "importpath", "modules",
                "module-auth", "packages", "private", "testflag", "testfunc", "vcs"]]},
        },
    }
}

fn cc() {
    #{
        single_dash: true,
        opts: `
            -o FILE  write the output to FILE
            -c  compile or assemble, but do not link
            -S  compile only; do not assemble or link
            -E  preprocess only; do not compile, assemble or link
            -g  produce debugging information
            -g3  produce debugging information, with macros
            -ggdb  produce debugging information for gdb
            -O0  do not optimize (the default)
            -O1  optimize
            -O2  optimize more
            -O3  optimize even more
            -Os  optimize for size
            -Oz  optimize for size aggressively
            -Og  optimize for debugging
            -Ofast  optimize, disregarding strict standards compliance
            -Wall  enable most warnings
            -Wextra  enable extra warnings
            -Werror  make all warnings errors
            -Wpedantic, -pedantic  warn about non-ISO constructs
            -Wshadow  warn when a variable shadows another
            -Wconversion  warn about implicit conversions that may change a value
            -Wno-unused-parameter  don't warn about unused parameters
            -w  inhibit all warnings
            -std=STANDARD  the language standard
            -x LANGUAGE  the language of the input files
            -I DIR  add DIR to the include search path
            -isystem DIR  add DIR to the system include search path
            -iquote DIR  add DIR to the search path of #include "..."
            -include FILE  include FILE before the source
            -L DIR  add DIR to the library search path
            -l LIBRARY  link with LIBRARY
            -D MACRO  define MACRO (as MACRO=VALUE, or 1)
            -U MACRO  undefine MACRO
            -march=CPU  generate code for CPU
            -mtune=CPU  tune the code for CPU
            -m32  generate code for a 32-bit environment
            -m64  generate code for a 64-bit environment
            -fPIC  generate position-independent code
            -fPIE  generate position-independent code for an executable
            -fno-omit-frame-pointer  keep the frame pointer
            -fsanitize=SANITIZER  enable a sanitizer
            -fstack-protector-strong  protect against stack smashing
            -fopenmp  enable OpenMP
            -ffast-math  allow unsafe floating point optimizations
            -funroll-loops  unroll loops
            -flto  enable link time optimization
            -fdiagnostics-color=WHEN  colour the diagnostics
            -fno-exceptions  disable exceptions
            -fno-rtti  disable run-time type information
            -shared  create a shared library
            -static  link statically
            -pie  create a position independent executable
            -pthread  link with the POSIX threads library
            -nostdlib  do not link with the standard libraries
            -rdynamic  export all symbols to the dynamic symbol table
            -M  output make rules of the dependencies
            -MM  like -M, without system headers
            -MD  write make rules of the dependencies to a .d file while compiling
            -MMD  like -MD, without system headers
            -MF FILE  write the dependencies to FILE
            -MT TARGET  the target of the dependency rule
            -MP  add a phony target for each dependency
            -save-temps  keep the intermediate files
            -pipe  use pipes rather than temporary files
            -v  print the commands run
            -dumpversion  print the version of the compiler
            -dumpmachine  print the target machine
            -print-search-dirs  print the search directories
            --version  print the version
            --help  print the help
        `,
        values: #{
            "-std": [
                "c89", "c99", "c11", "c17", "c23", "gnu89", "gnu99", "gnu11", "gnu17", "gnu23", "c++98", "c++11",
                "c++14", "c++17", "c++20", "c++23", "c++26", "gnu++11", "gnu++14", "gnu++17", "gnu++20", "gnu++23",
            ],
            "-x": ["c", "c++", "c-header", "c++-header", "objective-c", "assembler", "assembler-with-cpp", "none"],
            "-I": "dirs", "-isystem": "dirs", "-iquote": "dirs", "-L": "dirs", "-l": "none", "-D": "none",
            "-U": "none", "-march": ["native", "x86-64", "x86-64-v2", "x86-64-v3", "x86-64-v4", "armv8-a"],
            "-mtune": ["native", "generic"],
            "-fsanitize": ["address", "undefined", "thread", "memory", "leak", "fuzzer"],
            "-fdiagnostics-color": ["always", "auto", "never"], "-MT": "none",
        },
    }
}

fn cmake() {
    #{
        strict_eq: true,
        opts: `
            -S DIR  the source directory
            -B DIR  the build directory
            -C FILE  pre-load a script to populate the cache
            -D VAR=VALUE  create or update a cache entry
            -U GLOB  remove the matching entries from the cache
            -G GENERATOR  the build system generator
            -T TOOLSET  the toolset name, if the generator supports it
            -A PLATFORM  the platform name, if the generator supports it
            --toolchain FILE  the toolchain file
            --install-prefix DIR  the install directory
            -Wdev  enable developer warnings
            -Wno-dev  suppress developer warnings
            -Werror=dev  make developer warnings errors
            -Wdeprecated  enable deprecation warnings
            -Wno-deprecated  suppress deprecation warnings
            --preset=PRESET  the configure preset
            --list-presets[=TYPE]  list the presets
            -E  command mode
            --fresh  configure a fresh build tree
            --build DIR  build a project binary tree
            --install DIR  install a project binary tree
            --open DIR  open the project in the associated application
            -N  view mode only
            -P FILE  process a script
            --graphviz=FILE  generate a graphviz of the dependencies
            --system-information[=FILE]  dump information about this system
            --log-level=LEVEL  the verbosity of messages
            --log-context  prepend log messages with context
            --debug-output  put cmake in a debug mode
            --debug-find  put cmake find in a debug mode
            --trace  put cmake in trace mode
            --trace-expand  put cmake in trace mode, with variables expanded
            --warn-uninitialized  warn about uninitialized values
            --target, -t TARGET  with --build, the target to build
            -j, --parallel[=N]  with --build, the number of jobs
            --config CONFIG  with --build or --install, the configuration
            --clean-first  with --build, build the target clean first
            --prefix DIR  with --install, the installation prefix
            --component COMPONENT  with --install, the component to install
            -v, --verbose  verbose output
            --help  print the help
            --version  print the version
        `,
        values: #{
            "-S": "dirs", "-B": "dirs", "-D": "none", "-U": "none",
            "-G": [
                "Ninja", "Ninja Multi-Config", "Unix Makefiles", "Xcode", "Visual Studio 17 2022", "CodeBlocks - Ninja",
            ],
            "-T": "none", "-A": "none", "--install-prefix": "dirs", "--preset": "none",
            "--list-presets": ["configure", "build", "test", "package", "workflow", "all"],
            "--build": "dirs", "--install": "dirs", "--open": "dirs",
            "--log-level": ["ERROR", "WARNING", "NOTICE", "STATUS", "VERBOSE", "DEBUG", "TRACE"],
            "--target": "none", "-j": "none", "--config": ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"],
            "--prefix": "dirs", "--component": "none",
        },
        args: ["dirs"],
    }
}

fn meson() {
    let setup = #{
        opts: `
            --prefix PREFIX  the installation prefix
            --bindir DIR  the executable directory
            --libdir DIR  the library directory
            --buildtype TYPE  the build type
            --debug  enable debug symbols
            --optimization LEVEL  the optimization level
            -D OPTION=VALUE  set the value of an option
            --native-file FILE  a file describing the build machine
            --cross-file FILE  a file describing the cross compilation environment
            --reconfigure  set options and reconfigure the project
            --wipe  wipe the build directory and reconfigure
            --default-library TYPE  the default library type
            --backend BACKEND  the backend to use
            --werror  treat warnings as errors
            --warnlevel LEVEL  the compiler warning level
            --unity MODE  unity build
            -h, --help  show the help
        `,
        values: #{
            "--prefix": "dirs", "--bindir": "none", "--libdir": "none",
            "--buildtype": ["plain", "debug", "debugoptimized", "release", "minsize", "custom"],
            "--optimization": ["plain", "0", "g", "1", "2", "3", "s"], "-D": "none",
            "--default-library": ["shared", "static", "both"],
            "--backend": ["ninja", "vs", "vs2010", "vs2019", "vs2022", "xcode", "none"],
            "--warnlevel": ["0", "1", "2", "3", "everything"], "--unity": ["on", "off", "subprojects"],
        },
        args: ["dirs", "dirs"],
    };
    let dir = #{opts: "-C DIR  the build directory\n-h, --help  show the help", values: #{"-C": "dirs"}};
    #{
        commands: `
            setup  configure the project
            configure  change the options of a configured project
            compile  build a configured project
            test  run the tests
            install  install the project
            introspect  introspect a project
            init  create a new project
            dist  generate a release archive
            wrap  manage wraps
            subprojects  manage subprojects
            devenv  run commands in the developer environment
            format, fmt  format meson source files
            rewrite  modify the project definition
            env2mfile  convert the current environment to a cross or native file
            help  print the help of a command
        `,
        subs: #{
            setup: setup,
            configure: #{opts: setup.opts, values: setup.values, args: ["dirs"]},
            compile: #{
                opts: `
                    -C DIR  the build directory
                    -j, --jobs N  the number of jobs
                    -l, --load-average N  do not start new jobs while the load average is above N
                    --clean  clean the build directory
                    -v, --verbose  show more output
                    -h, --help  show the help
                `,
                values: #{"-C": "dirs", "-j": "none", "-l": "none"},
                args: ["none"],
            },
            test: #{
                opts: `
                    -C DIR  the build directory
                    --suite SUITE  run only the tests of SUITE
                    --no-suite SUITE  do not run the tests of SUITE
                    --no-rebuild  do not rebuild before running the tests
                    --gdb  run the tests under gdb
                    --list  list the tests
                    --wrapper WRAPPER  run the tests with a wrapper (such as valgrind)
                    --repeat N  run the tests N times
                    --num-processes N  the number of tests to run in parallel
                    -v, --verbose  show the output of the tests
                    -q, --quiet  show less output
                    -t, --timeout-multiplier N  multiply the timeouts by N
                    --print-errorlogs  print the logs of the tests that failed
                    -h, --help  show the help
                `,
                values: #{
                    "-C": "dirs", "--suite": "none", "--no-suite": "none", "--wrapper": "commands", "--repeat": "none",
                    "--num-processes": "none", "-t": "none",
                },
                args: ["none"],
            },
            install: dir,
            introspect: dir,
            dist: dir,
            devenv: #{opts: dir.opts, values: dir.values, args: ["commands", "files"]},
        },
    }
}

fn ninja() {
    #{
        opts: `
            --version  print ninja version
            -v, --verbose  show all command lines while building
            --quiet  don't show the progress, only the output of commands
            -C DIR  change to DIR before doing anything else
            -f FILE  specify the input build file
            -j N  run N jobs in parallel (0 means infinity)
            -k N  keep going until N jobs fail (0 means infinity)
            -l N  do not start new jobs if the load average is greater than N
            -n  dry run (don't run commands but act like they succeeded)
            -d MODE  enable debugging
            -t TOOL  run a subtool
            -w FLAG  adjust warnings
            -h  print the help
        `,
        values: #{
            "-C": "dirs", "-j": "none", "-k": "none", "-l": "none",
            "-d": ["stats", "explain", "keepdepfile", "keeprsp"],
            "-t": [
                "browse", "clean", "commands", "compdb", "deps", "graph", "inputs", "missingdeps", "query", "recompact",
                "restat", "rules", "targets", "cleandead", "urtle",
            ],
            "-w": ["dupbuild=err", "dupbuild=warn", "phonycycle=err", "phonycycle=warn"],
        },
        args: ["dev:ninja_targets"],
    }
}

fn gdb() {
    #{
        single_dash: true,
        opts: `
            --args  pass the arguments after the program to it
            --core=COREFILE, -c COREFILE  analyze the core dump COREFILE
            --exec=EXECFILE, -e EXECFILE  use EXECFILE as the executable
            --pid=PID, -p PID  attach to the running process PID
            --directory=DIR, -d DIR  search for source files in DIR
            --se=FILE  use FILE as the symbol file and the executable
            --symbols=SYMFILE, -s SYMFILE  read symbols from SYMFILE
            --readnow  fully read symbol files on first access
            --readnever  do not read symbol files
            --write  allow writing into executable and core files
            --command=FILE, -x FILE  run the gdb commands of FILE
            --init-command=FILE, -ix FILE  like -x, before loading the program
            --eval-command=COMMAND, -ex COMMAND  run a gdb command
            --init-eval-command=COMMAND, -iex COMMAND  like -ex, before loading the program
            --nh  do not read ~/.gdbinit
            --nx, -n  do not read any .gdbinit files
            --fullname  output information used by emacs-gdb
            --interpreter=INTERP  the interpreter or user interface
            --tty=TTY  use TTY for the input and output of the program
            --tui  use a terminal user interface
            -q, --quiet, --silent  do not print the version number on startup
            --batch  exit after processing options
            --batch-silent  like --batch, without gdb's output
            --return-child-result  exit with the status of the program
            --configuration  print details about the configuration of gdb and exit
            --help  print the help and exit
            --version  print the version and exit
            --cd=DIR  change the current directory to DIR
            --data-directory=DIR, -D DIR  set gdb's data directory to DIR
        `,
        values: #{
            "--pid": "pids", "--directory": "dirs", "--eval-command": "none", "--init-eval-command": "none",
            "--interpreter": ["console", "mi", "mi2", "mi3", "tui"], "--tty": "none", "--cd": "dirs",
            "--data-directory": "dirs",
        },
        args_if: #{"--args": ["files"]},
    }
}

fn python() {
    #{
        opts: `
            -b  warn about converting bytes to str and comparing them
            -B  don't write .pyc files on import
            -c CMD  run the program CMD (and end the options)
            -d  turn on parser debugging output
            -E  ignore the PYTHON* environment variables
            -h, -?, --help  print the help and exit
            -i  inspect interactively after running the script
            -I  isolate Python from the user's environment
            -m MOD  run the library module MOD as a script (and end the options)
            -O  remove assert and __debug__-dependent statements
            -OO  like -O, and remove docstrings
            -P  don't prepend a potentially unsafe path to sys.path
            -q  don't print the version and copyright messages on interactive startup
            -s  don't add the user site directory to sys.path
            -S  don't import site on initialization
            -u  force the stdout and stderr streams to be unbuffered
            -v  verbose (trace import statements)
            -V, --version  print the Python version and exit
            -W ARG  warning control
            -x  skip the first line of the source
            -X OPT  set an implementation-specific option
            --check-hash-based-pycs=MODE  control the validation of hash-based .pyc files
            --help-env  print help about the environment variables and exit
            --help-xoptions  print help about the -X options and exit
            --help-all  print the complete help and exit
        `,
        values: #{
            "-c": "none", "-m": "dev:python_modules",
            "-W": ["default", "error", "ignore", "always", "module", "once"],
            "-X": [
                "dev", "faulthandler", "importtime", "tracemalloc", "utf8", "showrefcount", "frozen_modules=off",
                "warn_default_encoding", "no_debug_ranges", "int_max_str_digits=", "perf", "gil=0",
            ],
            "--check-hash-based-pycs": ["always", "default", "never"],
        },
        args_if: #{"-c": ["files"], "-m": ["files"]},
    }
}

fn sqlite3() {
    #{
        single_dash: true,
        opts: `
            -A ARGS  run .archive ARGS and exit
            -append  append the database to the end of the file
            -ascii  set the output mode to ascii
            -bail  stop after hitting an error
            -batch  force batch I/O
            -box  set the output mode to box
            -column  set the output mode to column
            -cmd COMMAND  run COMMAND before reading stdin
            -csv  set the output mode to csv
            -deserialize  open the database using sqlite3_deserialize()
            -echo  print inputs before running them
            -init FILENAME  read and run FILENAME
            -header  turn headers on
            -noheader  turn headers off
            -help  show the help
            -html  set the output mode to HTML
            -interactive  force interactive I/O
            -json  set the output mode to json
            -line  set the output mode to line
            -list  set the output mode to list
            -markdown  set the output mode to markdown
            -newline SEP  set the output row separator
            -nofollow  refuse to open symbolic links to database files
            -nullvalue TEXT  set the text of NULL values
            -quote  set the output mode to quote
            -readonly  open the database read-only
            -safe  enable safe mode
            -separator SEP  set the output column separator
            -stats  print memory stats before each finalize
            -table  set the output mode to table
            -tabs  set the output mode to tabs
            -version  show the version of SQLite
            -vfs NAME  use NAME as the default VFS
            -zip  open the file as a ZIP archive
        `,
        values: #{"-A": "none", "-cmd": "none", "-newline": "none", "-nullvalue": "none", "-separator": "none",
            "-vfs": "none"},
        args: ["files", "none"],
    }
}

fn vim() {
    #{
        strict_eq: true,
        opts: `
            -v  vi mode (like vi)
            -e  ex mode (like ex)
            -E  improved Ex mode
            -s  silent (batch) mode (only for ex)
            -d  diff mode (like vimdiff)
            -y  easy mode (like evim, modeless)
            -R  readonly mode (like view)
            -Z  restricted mode (like rvim)
            -m  modifications (writing files) not allowed
            -M  modifications in text not allowed
            -b  binary mode
            -l  lisp mode
            -C  compatible with Vi: compatible
            -N  not fully Vi compatible: nocompatible
            -D  debugging mode
            -n  no swap file, use memory only
            -r  list swap files, or recover a crashed session
            -L  same as -r
            -o  open one window per file
            -O  open one window per file, side by side
            -p  open one tab page per file
            -q ERRORFILE  edit the file with the first error
            -t TAG  edit the file where TAG is defined
            -T TERMINAL  set the terminal type to TERMINAL
            --not-a-term  skip the warning for input or output not being a terminal
            --ttyfail  exit if input or output is not a terminal
            -u VIMRC  use VIMRC instead of any .vimrc
            --noplugin  don't load plugin scripts
            --cmd COMMAND  run COMMAND before loading any vimrc file
            -c COMMAND  run COMMAND after loading the first file
            -S SESSION  source the file SESSION after loading the first file
            -w SCRIPTOUT  append all typed commands to the file SCRIPTOUT
            -W SCRIPTOUT  write all typed commands to the file SCRIPTOUT
            -x  edit encrypted files
            --startuptime FILE  write startup timing messages to FILE
            --log FILE  start logging to FILE early
            -i VIMINFO  use VIMINFO instead of .viminfo
            --clean  nocompatible, Vim defaults, no plugins, no viminfo
            -h, --help  print the help and exit
            --version  print version information and exit
            --servername NAME  send to or become the Vim server NAME
            --remote  edit the files in a Vim server if possible
            --remote-silent  like --remote, without complaining if there is no server
            --remote-tab  like --remote, with a tab page per file
            --remote-send KEYS  send KEYS to a Vim server and exit
            --remote-expr EXPR  evaluate EXPR in a Vim server and print the result
            --serverlist  list the available Vim server names and exit
        `,
        values: #{
            "-t": "dev:tags", "-T": "none", "--cmd": "none", "-c": "none", "--servername": "none",
            "--remote-send": "none", "--remote-expr": "none",
        },
    }
}

fn nvim() {
    #{
        strict_eq: true,
        opts: `
            --cmd CMD  run CMD before any config
            -c CMD  run CMD after loading the first file
            -l SCRIPT  run the Lua script SCRIPT (with optional arguments)
            -S SESSION  source SESSION after loading the first file
            -s SCRIPTIN  read Normal mode commands from SCRIPTIN
            -u CONFIG  use this config file
            -d  diff mode
            -es, -Es  silent (batch) mode
            -e  ex mode
            -E  improved Ex mode
            -h, --help  print this help message
            -i SHADA  use this shada file
            -n  no swap file, use memory only
            -o  open one window per file
            -O  open one window per file, side by side
            -p  open one tab page per file
            -q ERRORFILE  edit the file with the first error
            -r  list swap files, or recover a crashed session
            -R  read-only (view) mode
            -t TAG  edit the file where TAG is defined
            -V  verbose
            -v, --version  print version information
            -x  edit encrypted files
            --api-info  write msgpack-encoded API metadata to stdout
            --clean  factory defaults (skip user config and plugins, shada)
            --embed  use stdin/stdout as a msgpack-rpc channel
            --headless  don't start a user interface
            --listen ADDRESS  serve the RPC API from this address
            --noplugin  don't load plugin scripts
            --remote  edit the files in a server
            --remote-send KEYS  send keys to a server
            --remote-expr EXPR  evaluate an expression in a server
            --server ADDRESS  the RPC server to send commands to
            --startuptime FILE  write startup timing messages to FILE
        `,
        values: #{
            "--cmd": "none", "-c": "none", "-t": "dev:tags", "--listen": "none", "--remote-send": "none",
            "--remote-expr": "none", "--server": "none",
        },
        args_if: #{"-l": ["files"]},
    }
}

fn nano() {
    #{
        opts: `
            -A, --smarthome  enable smart home key
            -B, --backup  save backups of existing files
            -C, --backupdir=DIR  directory for saving unique backup files
            -D, --boldtext  use bold instead of reverse video text
            -E, --tabstospaces  convert typed tabs to spaces
            -F, --multibuffer  read a file into a new buffer by default
            -G, --locking  use (vim-style) lock files
            -H, --historylog  save and reload old search and replace strings
            -I, --ignorercfiles  don't look at nanorc files
            -J, --guidestripe=NUMBER  show a guiding bar at this column
            -K, --rawsequences  fix numeric keypad key confusion problem
            -L, --nonewlines  don't add an automatic newline
            -M, --trimblanks  trim tail spaces when hard-wrapping
            -N, --noconvert  don't convert files from DOS/Mac format
            -O, --bookstyle  leading whitespace means new paragraph
            -P, --positionlog  save and restore the position of the cursor
            -Q, --quotestr=REGEX  regular expression to match quoting
            -R, --restricted  restrict access to the filesystem
            -S, --softwrap  display overlong lines on multiple rows
            -T, --tabsize=NUMBER  make a tab this number of columns wide
            -U, --quickblank  wipe status bar upon next keystroke
            -V, --version  print version information and exit
            -W, --wordbounds  detect word boundaries more accurately
            -X, --wordchars=STRING  which other characters are word parts
            -Y, --syntax=NAME  syntax definition to use for coloring
            -Z, --zap  let Bsp and Del erase a marked region
            -a, --atblanks  when soft-wrapping, do it at whitespace
            -b, --breaklonglines  automatically hard-wrap overlong lines
            -c, --constantshow  constantly show cursor position
            -d, --rebinddelete  fix Backspace/Delete confusion problem
            -e, --emptyline  keep the line below the title bar empty
            -f, --rcfile=FILE  use only this file for configuring nano
            -g, --showcursor  show cursor in file browser and help text
            -h, --help  show this help text and exit
            -i, --autoindent  automatically indent new lines
            -j, --jumpyscrolling  scroll per half-screen, not per line
            -k, --cutfromcursor  cut from cursor to end of line
            -l, --linenumbers  show line numbers in front of the text
            -m, --mouse  enable the use of the mouse
            -n, --noread  do not read the file (only write it)
            -o, --operatingdir=DIR  set operating directory
            -p, --preserve  preserve XON (^Q) and XOFF (^S) keys
            -q, --indicator  show a position and portion indicator
            -r, --fill=NUMBER  set width for hard-wrap and justify
            -s, --speller=PROGRAM  use this alternative spell checker
            -t, --saveonexit  save changes on exit, don't prompt
            -u, --unix  save a file by default in Unix format
            -v, --view  view mode (read-only)
            -w, --nowrap  don't hard-wrap long lines (the default)
            -x, --nohelp  don't show the two help lines
            -y, --afterends  make Ctrl+Right stop at word ends
            -%, --stateflags  show some states on the title bar
            -_, --minibar  show a feedback bar at the bottom
            -0, --zero  hide all bars, use the whole terminal
        `,
        guess_values: true,
        values: #{"-s": "commands"},
    }
}

fn emacs() {
    #{
        single_dash: true,
        opts: `
            -nw, --no-window-system  don't use the window system; use the terminal
            -q, --no-init-file  don't load the init file
            -Q, --quick  like -q --no-site-file --no-splash --no-x-resources
            --no-site-file  don't load site-start.el
            --no-splash  don't show the splash screen
            --batch  run in batch mode (implies -q)
            --script FILE  run FILE as an Emacs Lisp script
            -l FILE, --load=FILE  load the Emacs Lisp file FILE
            -f FUNCTION, --funcall=FUNCTION  call the Lisp function FUNCTION
            --eval=EXPR  evaluate the Lisp expression EXPR
            -L DIR, --directory=DIR  prepend DIR to load-path
            --chdir=DIR  change to DIR first
            --daemon  start a server in the background
            --fg-daemon  start a server in the foreground
            --debug-init  enable the debugger while loading the init file
            -t DEVICE, --terminal=DEVICE  use DEVICE as the terminal
            --init-directory=DIR  use DIR as the init directory
            --user=USER  load the init file of USER
            --version  print the version and exit
            --help  print the help and exit
        `,
        values: #{"-f": "none", "--eval": "none", "-L": "dirs", "--chdir": "dirs", "-t": "none",
            "--init-directory": "dirs", "--user": "users"},
    }
}

fn emacsclient() {
    #{
        opts: `
            -c, --create-frame  create a new frame
            -t, -nw, --tty  open a new frame on the terminal
            -n, --no-wait  don't wait for the server to return
            -a EDITOR, --alternate-editor=EDITOR  the editor to use if there is no server
            -s SOCKET, --socket-name=SOCKET  the socket of the server
            -e, --eval  evaluate the arguments as Lisp expressions
            -r, --reuse-frame  create a frame only if there is none
            -q, --quiet  don't show messages on success
            -u, --suppress-output  don't show the value of --eval
            -F ALIST, --frame-parameters=ALIST  the parameters of a new frame
            -d DISPLAY, --display=DISPLAY  the X display to use
            -V, --version  print the version and exit
            -H, --help  print the help and exit
        `,
        values: #{"-a": "commands", "-s": "none", "-F": "none", "-d": "none"},
    }
}

fn spec(cmd) {
    switch cmd {
        "cargo" => cargo(),
        "rustup" => rustup(),
        "go" => golang(),
        "gcc" | "g++" | "cc" | "c++" | "clang" | "clang++" => cc(),
        "cmake" => cmake(),
        "meson" => meson(),
        "ninja" => ninja(),
        "gdb" => gdb(),
        "python" | "python3" => python(),
        "sqlite3" => sqlite3(),
        "vim" | "vi" | "view" | "vimdiff" | "gvim" | "rvim" => vim(),
        "nvim" => nvim(),
        "nano" => nano(),
        "emacs" => emacs(),
        "emacsclient" => emacsclient(),
        _ => throw `no spec for ${cmd}`,
    }
}
