// Specs for the package managers of programming languages: pip, uv, conda
// (and mamba), pixi, npm, npx, yarn and pnpm (see lib.rhai for their
// fields). uv and pixi are read from their own `-h`.

import "kinds" as kinds;
import "lib" as lib;

// The name of a Python distribution from its `.dist-info` directory
// (`numpy-2.1.0.dist-info` is `numpy`).
fn dist_name(d) {
    let n = d.sub_string(0, d.len() - ".dist-info".len());
    let dash = n.index_of("-");
    if dash > 0 { n.sub_string(0, dash) } else { n }
}

// The Python packages installed where `python` (a path) finds them: the
// site-packages directories of its prefix, and the user's.
fn python_packages(python) {
    let dirs = [];
    let prefixes = [];
    if python != () {
        let bin = kinds::split_dir(python)[0];
        prefixes.push(kinds::split_dir(bin.sub_string(0, bin.len() - 1))[0]);
    }
    let venv = sh::getvar("VIRTUAL_ENV");
    if venv != () && venv != "" {
        prefixes.push(venv + "/");
    }
    for p in prefixes {
        for l in fs::list_dir(p + "lib") ?? [] {
            if l.starts_with("python") {
                dirs.push(p + "lib/" + l + "/site-packages");
            }
        }
    }
    for l in fs::list_dir(kinds::home() + "/.local/lib") ?? [] {
        if l.starts_with("python") {
            dirs.push(kinds::home() + "/.local/lib/" + l + "/site-packages");
        }
    }
    let out = [];
    for d in dirs {
        for e in fs::list_dir(d) ?? [] {
            if e.ends_with(".dist-info") {
                let n = dist_name(e);
                if !(n in out) {
                    out.push(n);
                }
            }
        }
    }
    out
}

// The name of a package in a requirement (`numpy>=2` is `numpy`).
fn requirement_name(r) {
    let r = kinds::unquote(r);
    let end = r.len();
    let k = 0;
    for c in r.chars() {
        if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.') {
            end = k;
            break;
        }
        k += 1;
    }
    r.sub_string(0, end)
}

// The dependencies of the nearest pyproject.toml.
fn pyproject_deps() {
    let p = fs::find_up("pyproject.toml");
    if p == () {
        return [];
    }
    let t = kinds::toml(fs::read_file(p) ?? "");
    let out = [];
    for r in kinds::toml_list((t["project"] ?? #{})["dependencies"] ?? "") {
        out.push(requirement_name(r));
    }
    for g in (t["dependency-groups"] ?? #{}).values() + (t["project.optional-dependencies"] ?? #{}).values() {
        for r in kinds::toml_list(g) {
            out.push(requirement_name(r));
        }
    }
    out
}

// The root of the conda installation.
fn conda_root() {
    for v in ["CONDA_EXE", "MAMBA_EXE"] {
        let exe = sh::getvar(v);
        if exe != () && exe != "" {
            let bin = kinds::split_dir(exe)[0];
            return kinds::split_dir(bin.sub_string(0, bin.len() - 1))[0];
        }
    }
    let root = sh::getvar("MAMBA_ROOT_PREFIX");
    if root != () && root != "" {
        return root + "/";
    }
    let conda = sh::which("conda") ?? sh::which("mamba");
    if conda == () {
        return ();
    }
    let bin = kinds::split_dir(conda)[0];
    kinds::split_dir(bin.sub_string(0, bin.len() - 1))[0]
}

// The names of the conda environments (and `base`).
fn conda_envs() {
    let out = ["base"];
    let root = conda_root();
    if root != () {
        for e in fs::list_dir(root + "envs") ?? [] {
            if !e.starts_with(".") {
                out.push(e);
            }
        }
    }
    for line in (fs::read_file(kinds::home() + "/.conda/environments.txt") ?? "").split("\n") {
        let n = kinds::split_dir(line)[1];
        if n != "" && !(n in out) && line.contains("/envs/") {
            out.push(n);
        }
    }
    out
}

// The packages installed in a conda environment: the one of `-n` or `-p`,
// or the active one.
fn conda_packages(words) {
    let prefix = kinds::option_value(words, "-p", "--prefix");
    let name = kinds::option_value(words, "-n", "--name");
    if prefix == () && name != () {
        let root = conda_root();
        prefix = if root == () { () } else if name == "base" { root } else { root + "envs/" + name };
    }
    prefix = prefix ?? sh::getvar("CONDA_PREFIX");
    if prefix == () || prefix == "" {
        return [];
    }
    let out = [];
    for f in fs::list_dir(kinds::tilde(prefix) + "/conda-meta") ?? [] {
        if !f.ends_with(".json") {
            continue;
        }
        // NAME-VERSION-BUILD.json
        let parts = f.sub_string(0, f.len() - 5).split("-");
        if parts.len() >= 3 {
            out.push(parts.extract(0, parts.len() - 2).reduce(|a, b| if a == "" { b } else { a + "-" + b }, ""));
        }
    }
    out
}

// The manifest of the pixi workspace: pixi.toml, or the tool.pixi tables
// of pyproject.toml, with the `tool.pixi.` taken out of their names.
fn pixi_manifest() {
    let p = fs::find_up("pixi.toml");
    if p != () {
        return kinds::toml(fs::read_file(p) ?? "");
    }
    let p = fs::find_up("pyproject.toml");
    if p == () {
        return #{};
    }
    let t = kinds::toml(fs::read_file(p) ?? "");
    let out = #{};
    for k in t.keys() {
        if k.starts_with("tool.pixi.") {
            out[k.sub_string(10)] = t[k];
        }
    }
    out
}

// The command of a pixi task: a string, or the `cmd` of an inline table.
fn task_command(v) {
    v.trim();
    if v.starts_with("{") {
        let k = v.index_of("cmd");
        if k < 0 {
            return "";
        }
        let rest = v.sub_string(k + 3);
        let eq = rest.index_of("=");
        rest = rest.sub_string(eq + 1);
        rest.trim();
        let q = rest[0];
        let end = rest.index_of(q, 1);
        return if end > 0 { rest.sub_string(1, end - 1) } else { "" };
    }
    kinds::unquote(v)
}

// The tasks of the pixi workspace (of all features and targets), with
// their commands as descriptions.
fn pixi_tasks() {
    let m = pixi_manifest();
    let out = [];
    let seen = #{};
    for k in m.keys() {
        if k == "tasks" || k.ends_with(".tasks") {
            for t in m[k].keys() {
                if !seen.contains(t) {
                    seen[t] = true;
                    out.push(#{value: t, desc: task_command(m[k][t])});
                }
            }
        } else if k.starts_with("tasks.") || k.contains(".tasks.") {
            // `[tasks.NAME]`
            let t = k.sub_string(k.index_of("tasks.") + 6);
            if !seen.contains(t) {
                seen[t] = true;
                out.push(#{value: t, desc: kinds::unquote(m[k]["cmd"] ?? "")});
            }
        }
    }
    out
}

fn pixi_envs() {
    let out = ["default"];
    for e in (pixi_manifest()["environments"] ?? #{}).keys() {
        if !(e in out) {
            out.push(e);
        }
    }
    out
}

fn pixi_deps() {
    let m = pixi_manifest();
    let out = [];
    for k in m.keys() {
        if k == "dependencies" || k.ends_with(".dependencies") || k.ends_with("pypi-dependencies") {
            for d in m[k].keys() {
                if !(d in out) {
                    out.push(d);
                }
            }
        }
    }
    out
}

// The nearest package.json, read.
fn package_json() {
    let p = fs::find_up("package.json");
    if p == () {
        return #{};
    }
    let r = #{};
    try {
        r = parse_json(fs::read_file(p) ?? "{}");
    } catch {
    }
    if type_of(r) == "map" { r } else { #{} }
}

// The scripts of package.json, with their commands.
fn npm_scripts() {
    let scripts = package_json().scripts ?? #{};
    let out = [];
    for k in scripts.keys() {
        out.push(#{value: k, desc: scripts[k]});
    }
    out
}

fn npm_deps() {
    let p = package_json();
    let out = [];
    for f in ["dependencies", "devDependencies", "optionalDependencies", "peerDependencies"] {
        for d in (p[f] ?? #{}).keys() {
            if !(d in out) {
                out.push(d);
            }
        }
    }
    out
}

// The commands of node_modules/.bin.
fn npm_bins() {
    let p = fs::find_up("node_modules");
    if p == () { [] } else { fs::list_dir(p + "/.bin") ?? [] }
}

fn kind(name, cur, words) {
    switch name {
        "pip_installed" => python_packages(sh::which(words[0])),
        "venv_installed" => python_packages(if fs::is_file(".venv/bin/python") { ".venv/bin/python" } else { () }),
        "pyproject_deps" => pyproject_deps(),
        "conda_envs" => conda_envs(),
        "conda_packages" => conda_packages(words),
        "pixi_tasks" => pixi_tasks(),
        "pixi_envs" => pixi_envs(),
        "pixi_deps" => pixi_deps(),
        "npm_scripts" => npm_scripts(),
        "npm_deps" => npm_deps(),
        "npm_bins" => npm_bins() + kinds::commands(cur),
        _ => throw `unknown kind: ${name}`,
    }
}

fn pip_general() {
    `
        -h, --help  show help
        --debug  let unhandled exceptions propagate outside the main subroutine
        --isolated  run pip in an isolated mode, ignoring environment variables and user configuration
        --require-virtualenv  allow pip to only run in a virtual environment
        --python <python>  run pip with the specified Python interpreter
        -v, --verbose  give more output
        -V, --version  show version and exit
        -q, --quiet  give less output
        --log <path>  path to a verbose appending log
        --no-input  disable prompting for input
        --keyring-provider <keyring_provider>  the mechanism of the credential lookup via the keyring library
        --proxy <proxy>  a proxy in the form scheme://[user:passwd@]proxy.server:port
        --retries <retries>  maximum number of retries each connection should attempt
        --timeout <sec>  set the socket timeout
        --exists-action <action>  default action when a path already exists
        --trusted-host <hostname>  mark this host or host:port pair as trusted
        --cert <path>  path to PEM-encoded CA certificate bundle
        --client-cert <path>  path to SSL client certificate
        --cache-dir <dir>  store the cache data in <dir>
        --no-cache-dir  disable the cache
        --disable-pip-version-check  don't check PyPI for a new version of pip
        --no-color  suppress colored output
        --use-feature <feature>  enable new functionality, that may be backward incompatible
        --use-deprecated <feature>  enable deprecated functionality, that will be removed in the future
    `
}

fn pip_index() {
    `
        -i, --index-url <url>  base URL of the Python Package Index
        --extra-index-url <url>  extra URLs of package indexes to use in addition to --index-url
        --no-index  ignore the package index (only look at --find-links URLs)
        -f, --find-links <url>  a URL or path to look for archives in
    `
}

fn pip_install() {
    `
        -r, --requirement <file>  install from the given requirements file
        -c, --constraint <file>  constrain versions using the given constraints file
        --no-deps  don't install package dependencies
        --pre  include pre-release and development versions
        -e, --editable <path/url>  install a project in editable mode
        --dry-run  don't actually install anything, just print what would be
        -t, --target <dir>  install packages into <dir>
        --platform <platform>  only use wheels compatible with <platform>
        --python-version <python_version>  the Python version to use for wheel and Requires-Python checks
        --implementation <implementation>  only use wheels compatible with a Python implementation
        --abi <abi>  only use wheels compatible with Python abi <abi>
        --user  install to the Python user install directory
        --root <dir>  install everything relative to this alternate root directory
        --prefix <dir>  installation prefix where lib, bin and other top-level folders are placed
        --src <dir>  directory to check out editable projects into
        -U, --upgrade  upgrade all specified packages to the newest available version
        --upgrade-strategy <upgrade_strategy>  how dependency upgrading should be handled
        --force-reinstall  reinstall all packages even if they are already up-to-date
        -I, --ignore-installed  ignore the installed packages, overwriting them
        --ignore-requires-python  ignore the Requires-Python information
        --no-build-isolation  disable isolation when building a modern source distribution
        --use-pep517  use PEP 517 for building source distributions
        --check-build-dependencies  check the build dependencies when PEP517 is used
        --break-system-packages  allow pip to modify an EXTERNALLY-MANAGED Python installation
        -C, --config-settings <settings>  configuration settings for the PEP 517 build backend
        --compile  compile Python source files to bytecode
        --no-compile  do not compile Python source files to bytecode
        --no-warn-script-location  do not warn when installing scripts outside PATH
        --no-warn-conflicts  do not warn about broken dependencies
        --no-binary <format_control>  do not use binary packages
        --only-binary <format_control>  do not use source packages
        --prefer-binary  prefer binary packages over source packages
        --require-hashes  require a hash to check each requirement against
        --progress-bar <progress_bar>  whether to show the progress bar
        --root-user-action <root_user_action>  the action if pip is run as root
        --report <file>  write a JSON file describing what pip did
        --no-clean  don't clean up build directories
    ` + pip_index()
}

fn pip() {
    let installed = #{args: ["langs:pip_installed"]};
    #{
        common: pip_general(),
        guess_values: true,
        values: #{
            "--keyring-provider": ["auto", "disabled", "import", "subprocess"],
            "--exists-action": ["switch", "ignore", "wipe", "backup", "abort"],
            "--upgrade-strategy": ["only-if-needed", "eager"], "--progress-bar": ["on", "off", "raw"],
            "--root-user-action": ["warn", "ignore"], "--no-binary": [":all:", ":none:"],
            "--only-binary": [":all:", ":none:"], "--format": ["columns", "freeze", "json"],
            "--exclude": "langs:pip_installed", "-e": "dirs", "--python": "files",
        },
        commands: `
            install  install packages
            download  download packages
            uninstall  uninstall packages
            freeze  output installed packages in requirements format
            inspect  inspect the python environment
            list  list installed packages
            show  show information about installed packages
            check  verify installed packages have compatible dependencies
            config  manage local and global configuration
            search  search PyPI for packages
            cache  inspect and manage pip's wheel cache
            index  inspect information available from package indexes
            wheel  build wheels from your requirements
            hash  compute hashes of package archives
            completion  a helper command used for command completion
            debug  show information useful for debugging
            help  show help for commands
        `,
        subs: #{
            install: #{opts: pip_install(), args: ["none"]},
            download: #{
                opts: pip_install() + "-d, --dest <dir>  download packages into <dir>",
                args: ["none"],
            },
            wheel: #{
                opts: pip_install() + "-w, --wheel-dir <dir>  build wheels into <dir>",
                args: ["none"],
            },
            uninstall: #{
                opts: `
                    -r, --requirement <file>  uninstall all the packages listed in the given requirements file
                    -y, --yes  don't ask for confirmation of uninstall deletions
                    --root-user-action <root_user_action>  the action if pip is run as root
                    --break-system-packages  allow pip to modify an EXTERNALLY-MANAGED Python installation
                `,
                args: ["langs:pip_installed"],
            },
            list: #{
                opts: `
                    -o, --outdated  list outdated packages
                    -u, --uptodate  list uptodate packages
                    -e, --editable  list editable projects
                    -l, --local  do not list globally-installed packages in a virtualenv
                    --user  only output packages installed in user-site
                    --path <path>  restrict to the specified installation path
                    --pre  include pre-release and development versions
                    --format <list_format>  the output format: columns, freeze or json
                    --not-required  list packages that are not dependencies of installed packages
                    --exclude-editable  exclude editable packages from the output
                    --include-editable  include editable packages in the output
                    --exclude <package>  exclude the package from the output
                ` + pip_index(),
                args: ["none"],
            },
            show: #{opts: "-f, --files  show the full list of installed files of each package", args: installed.args},
            freeze: #{
                opts: `
                    -r, --requirement <file>  use the order in the given requirements file and its comments
                    -l, --local  do not output globally-installed packages in a virtualenv
                    --user  only output packages installed in user-site
                    --path <path>  restrict to the specified installation path
                    --all  do not skip setuptools, distribute, wheel and pip in the output
                    --exclude-editable  exclude editable packages from the output
                    --exclude <package>  exclude the package from the output
                `,
                args: ["none"],
            },
            check: #{args: ["none"]},
            inspect: #{opts: `
                    --local  only packages in the virtualenv
                    --user  only packages in user-site
                `, args: ["none"]},
            config: #{
                opts: `
                    --editor <editor>  the editor to use to edit the file
                    --global  use the system-wide configuration file only
                    --user  use the user configuration file only
                    --site  use the current environment configuration file only
                `,
                commands: `
                    list  list the active configuration
                    edit  edit the configuration file in an editor
                    get  get the value associated with a name
                    set  set the value of a name
                    unset  unset the value associated with a name
                    debug  list the configuration files and values defined under them
                `,
            },
            cache: #{commands: `
                dir  show the cache directory
                info  show information about the cache
                list  list the filenames of the packages stored in the cache
                remove  remove one or more packages from the cache
                purge  remove all items from the cache
            `},
            index: #{commands: "versions  list the available versions of a package", opts: pip_index()},
            search: #{args: ["none"]},
            hash: #{opts: `
                    -a, --algorithm <algorithm>  the hash algorithm to use
                `, values: #{"-a": ["sha256", "sha384", "sha512"]}},
            completion: #{
                opts: `
                    -b, --bash  completion for bash
                    -z, --zsh  completion for zsh
                    -f, --fish  completion for fish
                    -p, --powershell  completion for powershell
                `,
                args: ["none"],
            },
            help: #{args: [[
                "install", "download", "uninstall", "freeze", "inspect", "list", "show", "check", "config", "search",
                "cache", "index", "wheel", "hash", "completion", "debug",
            ]]},
        },
    }
}

// uv, from its `-h`, with the arguments `-h` doesn't give.
fn uv(path) {
    let s = lib::help_spec(lib::help_of(path));
    s.guess_values = true;
    s.sub_spec = "langs:" + path;
    s.values.mixin(#{"--python": "none", "-p": "none", "--package": "none", "--with": "none", "--from": "none"});
    switch path {
        "uv remove" => { s.args = ["langs:pyproject_deps"]; }
        "uv pip uninstall" | "uv pip show" => { s.args = ["langs:venv_installed"]; }
        "uv add" | "uv pip install" | "uv tool install" | "uv python install" | "uv init" => {
            s.args = ["none"];
        }
        "uv run" | "uv tool run" => { s.args = ["commands", "files"]; }
        _ => (),
    }
    s
}

// pixi, from its `-h`, with the arguments `-h` doesn't give.
fn pixi(path) {
    let s = lib::help_spec(lib::help_of(path));
    s.guess_values = true;
    s.sub_spec = "langs:" + path;
    s.values.mixin(#{"-e": "langs:pixi_envs", "--environment": "langs:pixi_envs"});
    switch path {
        "pixi run" => { s.args = ["langs:pixi_tasks", "files"]; }
        "pixi remove" => { s.args = ["langs:pixi_deps"]; }
        "pixi task remove" | "pixi task alias" => { s.args = ["langs:pixi_tasks"]; }
        "pixi add" | "pixi search" | "pixi global install" | "pixi exec" | "pixi upgrade" | "pixi update" => {
            s.args = ["none"];
        }
        _ => (),
    }
    s
}

fn sub_spec(path, sub) {
    let p = path + " " + sub;
    if path.starts_with("uv") { uv(p) } else { pixi(p) }
}

fn conda() {
    let env = `
        -n, --name ENVIRONMENT  the name of the environment
        -p, --prefix PATH  the full path to the environment location
    `;
    let channels = `
        -c, --channel CHANNEL  an additional channel to search for packages
        --use-local  use locally built packages
        --override-channels  do not search the default or .condarc channels
        --repodata-fn REPODATA_FNS  the repodata files to use
        --strict-channel-priority  packages in lower priority channels are not considered
        --no-channel-priority  package version takes precedence over channel priority
    `;
    let output = `
        --json  report all output as JSON
        -v, --verbose  more output
        -q, --quiet  do not display progress bar
        -d, --dry-run  only display what would have been done
        -y, --yes  do not ask for confirmation
        --offline  offline mode
    `;
    let solver = `
        --file FILE  read package versions from FILE
        --no-deps  do not install, update, remove or change dependencies
        --only-deps  only install dependencies
        --freeze-installed, --no-update-deps  do not update or change already-installed dependencies
        --update-deps  update dependencies
        -S, --satisfied-skip-solve  exit early if the requested specs are satisfied
        --update-all, --all  update all installed packages in the environment
        --update-specs  update based on the provided specifications
        --force-reinstall  reinstall packages even if they are already installed
        --copy  install all packages using copies instead of hard or soft links
        --solver SOLVER  the solver to use
        --download-only  solve and download the packages, without installing them
        --clobber  allow clobbering of overlapping file paths
    `;
    let install = #{opts: env + channels + output + solver, args: ["none"]};
    let packages = #{opts: env + channels + output + `
        --all  remove all packages, i.e., the entire environment
        --keep-env  with --all, keep the environment directory
        --features  remove features (instead of packages)
        --force, --force-remove  remove the packages without removing their dependents
    `, args: ["langs:conda_packages"]};
    #{
        opts: `
            -h, --help  show the help message and exit
            -V, --version  show the conda version number and exit
        `,
        values: #{
            "-n": "langs:conda_envs", "-c": ["conda-forge", "bioconda", "defaults", "nvidia", "pytorch"],
            "--solver": ["classic", "libmamba"], "--repodata-fn": "none", "-p": "dirs",
        },
        commands: `
            activate  activate an environment
            deactivate  deactivate the current environment
            clean  remove unused packages and caches
            compare  compare packages between environments
            config  modify configuration values in .condarc
            create  create a new environment
            env  manage environments
            info  display information about the current conda install
            init  initialize conda for shell interaction
            install  install packages into an environment
            list  list the packages installed in an environment
            notices  retrieve the latest channel notifications
            package  low-level package utility
            remove, uninstall  remove packages from an environment
            rename  rename an environment
            run  run a command in an environment
            search  search for packages
            update, upgrade  update packages to the latest compatible version
            doctor  display a health report for an environment
        `,
        subs: #{
            activate: #{opts: "--stack  stack the environment on the current one\n--no-stack  don't stack it",
                args: ["langs:conda_envs"]},
            deactivate: #{args: ["none"]},
            install: install,
            update: #{opts: install.opts, args: ["langs:conda_packages"]},
            create: #{opts: install.opts + "--clone ENV  create an environment as a copy of ENV", values: #{
                "--clone": "langs:conda_envs"}, args: ["none"]},
            remove: packages,
            list: #{opts: env + `
                --show-channel-urls  show the channel URLs
                -c, --canonical  output the canonical names of packages only
                -f, --full-name  only search for full names
                --explicit  list explicitly all installed conda packages with URLs
                --md5  add the MD5 hash to the --explicit output
                -e, --export  output a requirement string only
                -r, --revisions  list the revision history
                --json  report all output as JSON
            `, args: ["none"]},
            search: #{opts: channels + `
                -i, --info  provide detailed information about each package
                --subdir, --platform SUBDIR  search the given subdir
                --envs  search all environments of the current user
                --json  report all output as JSON
            `, args: ["none"]},
            run: #{opts: env + `
                --cwd CWD  the current working directory of the command
                --no-capture-output, --live-stream  don't capture stdout and stderr
            `, values: #{"--cwd": "dirs"}, args: ["commands", "files"]},
            rename: #{opts: env + "--force  force the rename\n-d, --dry-run  only display what would be done",
                args: ["none"]},
            clean: #{opts: output + `
                -a, --all  remove the index cache, lock files, unused cache packages, tarballs and log files
                -i, --index-cache  remove the index cache
                -p, --packages  remove unused packages from the writable package caches
                -t, --tarballs  remove cached package tarballs
                -f, --force-pkgs-dirs  remove all writable package caches
                -c, --tempfiles  remove temporary files
                -l, --logfiles  remove log files
            `, args: ["none"]},
            info: #{opts: `
                    -a, --all  show all information
                    -e, --envs  list all known environments
                    --json  report all output as JSON
                    --base  display the base environment path
                `,
                args: ["none"]},
            env: #{
                commands: `
                    create  create an environment from an environment file
                    export  export a given environment
                    list  list the environments
                    remove  remove an environment
                    update  update an environment from an environment file
                    config  configure an environment
                `,
                subs: #{
                    create: #{opts: env + "-f, --file FILE  the environment file", args: ["none"]},
                    "export": #{opts: env + `
                            -f, --file FILE  write to FILE
                            --from-history  only the packages that were asked for
                            --no-builds  remove the build specifications
                        `,
                        args: ["none"]},
                    remove: #{opts: env + output, args: ["none"]},
                    update: #{opts: env + `
                            -f, --file FILE  the environment file
                            --prune  remove installed packages not in the file
                        `,
                        args: ["none"]},
                    list: #{args: ["none"]},
                },
            },
            config: #{opts: `
                --system  write to the system .condarc file
                --env  write to the active conda environment .condarc file
                --file FILE  write to the given file
                --show [KEY]  display configuration values
                --show-sources  display all identified configuration sources
                --validate  validate all configuration sources
                --describe [KEY]  describe the given configuration parameters
                --write-default  write the default configuration to a file
                --get [KEY]  get a configuration value
                --append KEY VALUE  add one configuration value to the end of a list key
                --prepend, --add KEY VALUE  add one configuration value to the beginning of a list key
                --set KEY VALUE  set a boolean or string key
                --remove KEY VALUE  remove a configuration value from a list key
                --remove-key KEY  remove a configuration key (and all its values)
            `, args: ["none"]},
            init: #{opts: `
                    --all  initialize all currently available shells
                    --reverse  undo effects of the last conda init
                    -d, --dry-run  only display what would have been done
                `,
                args: [["bash", "fish", "powershell", "tcsh", "xonsh", "zsh"]]},
        },
    }
}

fn npm_common() {
    `
        -w, --workspace <name>  run in the workspace <name>
        -ws, --workspaces  run in all workspaces
        --include-workspace-root  include the workspace root with --workspaces
        -g, --global  operate in global mode
        --registry <url>  the base URL of the npm registry
        --loglevel <level>  the level of logs to report
        -s, --silent  like --loglevel silent
        -d  like --loglevel info
        --json  output JSON
        --prefix <path>  the location to install global items
        --dry-run  report what would be done, without changing anything
        -y, --yes  answer yes to any prompts
        -h, --help  show the help
        -v, --version  show the version
    `
}

fn npm_save() {
    `
        -P, --save-prod  save as a dependency (the default)
        -D, --save-dev  save as a devDependency
        -O, --save-optional  save as an optionalDependency
        --save-peer  save as a peerDependency
        -B, --save-bundle  save as a bundleDependency
        -E, --save-exact  save the exact version rather than a range
        --no-save  don't save to package.json
        --legacy-peer-deps  ignore peerDependencies
        --strict-peer-deps  fail on conflicting peerDependencies
        --force  force the install
        --ignore-scripts  don't run the scripts of package.json
        --prefer-offline  use cached data when possible
        --prefer-online  check the registry for cached data
        --offline  never use the network
        --omit <type>  the dependency types to omit on disk
        --include <type>  the dependency types to install
        --install-strategy <strategy>  how to lay out node_modules
        --no-audit  don't submit audit reports
        --no-fund  don't show funding messages
        --package-lock-only  only update package-lock.json
    `
}

fn npm() {
    let deps = #{opts: npm_save(), args: ["langs:npm_deps"]};
    #{
        common: npm_common(),
        guess_values: true,
        values: #{
            "--loglevel": ["silent", "error", "warn", "notice", "http", "info", "verbose", "silly"],
            "--omit": ["dev", "optional", "peer"], "--include": ["prod", "dev", "optional", "peer"],
            "--install-strategy": ["hoisted", "nested", "shallow", "linked"],
        },
        commands: `
            access  set access level on published packages
            adduser, login  add a registry user account
            audit  run a security audit
            bugs  report bugs for a package in a web browser
            cache  manipulate the package cache
            ci, clean-install  clean install a project
            completion  tab completion for npm
            config, c  manage the npm configuration files
            dedupe, ddp  reduce duplication in the package tree
            deprecate  deprecate a version of a package
            diff  the registry diff command
            dist-tag  modify package distribution tags
            docs, home  open the documentation of a package in a web browser
            doctor  check the health of your npm environment
            edit  edit an installed package
            exec, x  run a command from a local or remote npm package
            explain, why  explain installed packages
            explore  browse an installed package
            find-dupes  find duplication in the package tree
            fund  retrieve funding information
            help  get help on npm
            help-search  search the npm help documentation
            init, create  create a package.json file
            install, i, add  install a package
            install-ci-test, cit  install a project with a clean slate and run the tests
            install-test, it  install a package and run the tests
            link, ln  symlink a package folder
            logout  log out of the registry
            ls, list  list installed packages
            org  manage orgs
            outdated  check for outdated packages
            owner  manage package owners
            pack  create a tarball from a package
            ping  ping the npm registry
            pkg  manage your package.json
            prefix  display the prefix
            profile  change settings on your registry profile
            prune  remove extraneous packages
            publish  publish a package
            query  retrieve a filtered list of packages
            rebuild, rb  rebuild a package
            repo  open the repository page of a package in the browser
            restart  restart a package
            root  display the npm root
            run, run-script, rum, urn  run the arbitrary package scripts
            sbom  generate a Software Bill of Materials
            search, find, s, se  search for packages
            shrinkwrap  lock down dependency versions for publication
            star  mark your favorite packages
            stars  view the packages marked as favorites
            start  start a package
            stop  stop a package
            team  manage organization teams and team memberships
            test, t, tst  test a package
            token  manage your authentication tokens
            uninstall, un, remove, rm, r, unlink  remove a package
            unpublish  remove a package from the registry
            unstar  remove an item from your favorite packages
            update, up, upgrade  update packages
            version  bump a package version
            view, info, show, v  view registry info
            whoami  display the npm username
        `,
        subs: #{
            install: #{opts: npm_save(), args: ["none"]},
            ci: #{opts: npm_save(), args: ["none"]},
            uninstall: deps,
            update: deps,
            outdated: #{args: ["langs:npm_deps"]},
            ls: #{opts: `
                    --all  show all packages, not only the direct dependencies
                    --depth <depth>  the depth to show
                    --long  show more information
                    --parseable  output parseable results
                `,
                args: ["langs:npm_deps"]},
            explain: #{args: ["langs:npm_deps"]},
            run: #{opts: `
                    --if-present  don't fail if the script is missing
                    --ignore-scripts  don't run pre and post scripts
                    --script-shell <path>  the shell to run the script with
                `,
                args: ["langs:npm_scripts", "files"]},
            exec: #{opts: `
                    --package <package>  the package to install and run
                    -c, --call <command>  the command to run
                `,
                values: #{"--package": "none", "-c": "none"}, args: ["langs:npm_bins", "files"]},
            view: #{args: ["langs:npm_deps", "none"]},
            docs: #{args: ["langs:npm_deps"]},
            repo: #{args: ["langs:npm_deps"]},
            bugs: #{args: ["langs:npm_deps"]},
            search: #{args: ["none"]},
            config: #{commands: `
                set  set a configuration value
                get  get a configuration value
                delete  delete a configuration value
                list  list the configuration
                edit  edit the configuration file
                fix  fix the configuration
            `},
            cache: #{commands: `
                    add  add a package to the cache
                    clean  delete the cache
                    ls  list the cache
                    verify  verify the cache
                `},
            version: #{args: [[
                "major", "minor", "patch", "premajor", "preminor", "prepatch", "prerelease", "from-git",
            ]]},
            help: #{args: [[
                "install", "run", "test", "update", "uninstall", "publish", "config", "init", "exec", "ls",
            ]]},
        },
    }
}

fn npx() {
    #{
        opts: `
            -p, --package <package>  the package to install and run
            -c, --call <command>  the command to run
            -y, --yes  install the package without asking
            --no  don't install packages that are missing
            -h, --help  show the help
        `,
        values: #{"-p": "none", "-c": "none"},
        args: ["langs:npm_bins", "files"],
    }
}

// The commands of yarn or pnpm, and the scripts of package.json, which
// they run by their names.
fn with_scripts(commands) {
    for s in npm_scripts() {
        commands += "\n" + s.value + "  " + s.desc;
    }
    commands
}

fn yarn() {
    let add = `
        -D, --dev  save as a devDependency
        -P, --peer  save as a peerDependency
        -O, --optional  save as an optionalDependency
        -E, --exact  save the exact version
        -T, --tilde  save with a ~ range
    `;
    #{
        opts: `
            -h, --help  show the help
            -v, --version  show the version
            --cwd <dir>  run in <dir>
            --silent  don't print anything
            --offline  never use the network
            --verbose  show more output
        `,
        guess_values: true,
        commands: with_scripts(`
            add  add dependencies to the project
            bin  get the path of a binary script
            cache  manage the cache
            config  manage the configuration
            create  create a new project from a create-* package
            dlx  run a package in a temporary environment
            exec  run a shell command
            global  manage global packages
            info  see information about a package
            init  create a new package
            install  install all the dependencies of the project
            link  connect the local project to another one
            list  list installed packages
            node  run node with the hook already set up
            npm  npm-related commands
            pack  create an archive of the package
            plugin  manage plugins
            publish  publish the package to the registry
            rebuild  rebuild the native packages
            remove  remove dependencies from the project
            run  run a script
            unlink  disconnect the local project from another one
            up, upgrade  upgrade dependencies
            upgrade-interactive  upgrade dependencies interactively
            version  change the version of the package
            why  display the reason why a package is needed
            workspace  run a command within the specified workspace
            workspaces  manage the workspaces
        `),
        subs: #{
            add: #{opts: add, args: ["none"]},
            remove: #{args: ["langs:npm_deps"]},
            up: #{opts: add, args: ["langs:npm_deps"]},
            why: #{args: ["langs:npm_deps"]},
            info: #{args: ["langs:npm_deps"]},
            run: #{args: ["langs:npm_scripts", "files"]},
            exec: #{args: ["commands", "files"]},
            dlx: #{args: ["none"]},
            install: #{opts: `
                    --frozen-lockfile  don't generate a lockfile, and fail if it needs changes
                    --immutable  fail if the lockfile needs changes
                `,
                args: ["none"]},
        },
    }
}

fn pnpm() {
    let filter = `
        -F, --filter <selector>  restrict the command to packages matching <selector>
        -r, --recursive  run in every project of the workspace
        -w, --workspace-root  run on the root of the workspace
    `;
    let add = filter + `
        -D, --save-dev  save as a devDependency
        -P, --save-prod  save as a dependency
        -O, --save-optional  save as an optionalDependency
        -E, --save-exact  save the exact version
        --save-peer  save as a peerDependency
        -g, --global  install globally
    `;
    #{
        opts: `
            -h, --help  show the help
            -v, --version  show the version
            -C, --dir <dir>  run as if pnpm was started in <dir>
            --silent, -s  don't print anything
            --reporter <name>  the output reporter
        `,
        guess_values: true,
        values: #{"--reporter": ["silent", "default", "append-only", "ndjson"], "-F": "none"},
        commands: with_scripts(`
            add  install a package and its dependencies
            install, i  install all the dependencies of the project
            update, up, upgrade  update packages to their latest versions
            remove, rm, uninstall, un  remove packages
            link, ln  connect the local project to another one
            unlink  unlink a package
            import  generate pnpm-lock.yaml from another lockfile
            rebuild, rb  rebuild a package
            prune  remove unnecessary packages
            fetch  fetch packages from a lockfile into the store
            install-test, it  run pnpm install and then pnpm test
            dedupe  deduplicate dependencies
            patch  prepare a package for patching
            patch-commit  generate a patch from a directory
            audit  check for known security issues
            list, ls  list installed packages
            outdated  check for outdated packages
            why  show the packages that depend on a package
            licenses  check the licenses of the installed packages
            run, run-script  run a script
            test, t, tst  run the test script
            exec  run a shell command in the context of a project
            dlx  fetch a package and run its command
            create  create a project from a create-* package
            start  run the start script
            publish  publish a package to the registry
            pack  create an archive of the package
            recursive, m, multi  run a command in every project of the workspace
            server  manage a store server
            store  manage the package store
            root  print the modules directory
            bin  print the directory of executables
            env  manage Node.js versions
            setup  set up pnpm
            init  create a package.json file
            deploy  deploy a package from a workspace
            config, c  manage the configuration
        `),
        subs: #{
            add: #{opts: add, args: ["none"]},
            install: #{opts: filter + `
                    --frozen-lockfile  don't update the lockfile
                    --prod, -P  only install dependencies
                    --dev, -D  only install devDependencies
                    --offline  never use the network
                `,
                args: ["none"]},
            update: #{opts: add + `
                    -L, --latest  update to the latest versions
                    -i, --interactive  choose the packages to update
                `,
                args: ["langs:npm_deps"]},
            remove: #{opts: add, args: ["langs:npm_deps"]},
            why: #{opts: filter, args: ["langs:npm_deps"]},
            list: #{opts: filter + `
                    --depth <number>  the depth to show
                    --prod, -P  only dependencies
                    --dev, -D  only devDependencies
                `,
                args: ["langs:npm_deps"]},
            outdated: #{opts: filter, args: ["langs:npm_deps"]},
            run: #{opts: filter + "--if-present  don't fail if the script is missing\n--parallel  run in parallel",
                args: ["langs:npm_scripts", "files"]},
            exec: #{opts: filter, args: ["commands", "files"]},
            dlx: #{args: ["none"]},
        },
    }
}

fn spec(cmd) {
    switch cmd {
        "pip" | "pip3" => pip(),
        "uv" => uv("uv"),
        "conda" | "mamba" | "micromamba" => conda(),
        "pixi" => pixi("pixi"),
        "npm" => npm(),
        "npx" => npx(),
        "yarn" => yarn(),
        "pnpm" => pnpm(),
        _ => throw `no spec for ${cmd}`,
    }
}
